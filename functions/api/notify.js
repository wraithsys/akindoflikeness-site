/* Opt-in mailing list endpoint (Cloudflare Pages Function).
 * POST {email} -> stored in the KV namespace `akol-notify`, which this
 * project binds as KV_BINDING. A Function only ever sees the BINDING name,
 * never the namespace name, so reading env.NOTIFY alone found nothing and
 * every signup took the unbound path below. Both names are accepted now.
 * No cadence, no third party: emails land in your own KV, you mail the list
 * only when a build ships. One-time setup: create a KV namespace and bind it
 * to this Pages project as `NOTIFY` (dashboard: Settings > Functions > KV
 * bindings).
 *
 * Each subscriber is stored as two keys, so a token in an unsubscribe link
 * (functions/api/unsubscribe.js) can find its owner without scanning the
 * whole list:
 *   sub:<email>  -> { ts: ISO-8601, token }   the record
 *   tok:<token>  -> <email>                    the index unsubscribe reads
 *
 * "Until it's bound the form still returns ok" was true and also a bug: an
 * unbound NOTIFY silently ate every signup with no trace anywhere. That is
 * still the right answer for the VISITOR (a broken-looking form is worse
 * than a form that quietly does nothing this once), but it must not be
 * silent for the operator — hence the console.error and the response
 * header below, which `notify-export.js`'s "not configured" case and any
 * Workers log both surface.
 */
const json = (o, s = 200, headers = {}) =>
  new Response(JSON.stringify(o), { status: s, headers: { "content-type": "application/json", ...headers } });

function randomToken() {
  const bytes = new Uint8Array(20);
  crypto.getRandomValues(bytes);
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}

// Old records were a bare ISO string, not JSON. Read either shape so a
// resubscribe (which rewrites the record) is the only migration needed.
function parseRecord(raw) {
  if (raw == null) return null;
  try {
    const v = JSON.parse(raw);
    if (v && typeof v === "object") return v;
  } catch {
    /* fall through to the legacy shape below */
  }
  return { ts: raw, token: null };
}

export async function onRequestPost({ request, env }) {
  try {
    const body = await request.json();
    const email = String(body.email || "").trim().toLowerCase();
    // one honest validation, no over-engineering
    if (email.length > 200 || !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) {
      return json({ ok: false, error: "that doesn't look like an email" }, 400);
    }

    const kv = env.NOTIFY || env.KV_BINDING;
    if (!kv) {
      // Not silent: this is the bug the research flagged (site-commerce.md
      // §6) — the KV binding can be missing in a preview/dev environment or
      // simply not yet wired up. The visitor is not told; the operator can
      // see it in the Functions log and via this header.
      console.error("notify: NOTIFY KV is not bound — signup was not stored", { email });
      return json({ ok: true }, 200, { "x-notify-store": "unbound" });
    }

    const subKey = "sub:" + email;
    // Resubscribing (or a double click) must not orphan the previous
    // token's index entry — clean it up before writing the new one.
    const existing = parseRecord(await kv.get(subKey));
    if (existing?.token) {
      await kv.delete("tok:" + existing.token);
    }

    const token = randomToken();
    const ts = new Date().toISOString();
    await kv.put(subKey, JSON.stringify({ ts, token }));
    await kv.put("tok:" + token, email);

    return json({ ok: true });
  } catch {
    return json({ ok: false, error: "something went wrong" }, 400);
  }
}
