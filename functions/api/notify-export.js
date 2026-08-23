/* Export the mailing list as CSV (Cloudflare Pages Function). Admin only.
 *
 * GET, header `x-notify-admin: <secret>` must match the `NOTIFY_ADMIN`
 * environment variable/secret. Set it once: Pages dashboard -> Settings ->
 * Environment variables -> add `NOTIFY_ADMIN` (mark it Secret, not plain
 * text) for the Production environment. Nothing in this repo can set it —
 * it is Billy's hands, same as the KV binding itself.
 *
 * Before this file existed the only way to read the list at all was
 * `wrangler kv key list --prefix sub:` from a machine with deploy access
 * (research/site-commerce.md §6). This is the same read, over HTTP, behind
 * a secret instead of a wrangler session.
 *
 * Unlike notify.js's public endpoint, a clear failure here is correct, not
 * a bug: this is never called by a visitor's browser, so there is no "ok
 * for the user" case to protect. "Not configured" means exactly that.
 */
function csvField(v) {
  const s = String(v ?? "");
  return /[",\n]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s;
}

function parseRecord(raw) {
  if (raw == null) return { ts: "", token: "" };
  try {
    const v = JSON.parse(raw);
    if (v && typeof v === "object") return { ts: v.ts ?? "", token: v.token ?? "" };
  } catch {
    /* legacy plain-string record: just a timestamp, no token yet */
  }
  return { ts: raw, token: "" };
}

export async function onRequestGet({ request, env }) {
  if (!env.NOTIFY_ADMIN) {
    return new Response("not configured: NOTIFY_ADMIN secret is not set for this deploy\n", {
      status: 501,
      headers: { "content-type": "text/plain; charset=utf-8" },
    });
  }

  const given = request.headers.get("x-notify-admin") || "";
  // Fixed-length-ish check is not the point at this scale; a plain compare
  // against a long random secret is the same posture the License API docs
  // describe for a shared secret at this traffic level.
  if (given !== env.NOTIFY_ADMIN) {
    return new Response("unauthorized\n", { status: 401, headers: { "content-type": "text/plain; charset=utf-8" } });
  }

  if (!env.NOTIFY) {
    return new Response("not configured: NOTIFY KV is not bound for this deploy\n", {
      status: 503,
      headers: { "content-type": "text/plain; charset=utf-8" },
    });
  }

  const rows = ["email,subscribed_at,unsubscribe_token"];
  let cursor;
  do {
    const listed = await env.NOTIFY.list({ prefix: "sub:", cursor });
    for (const key of listed.keys) {
      const email = key.name.slice("sub:".length);
      const { ts, token } = parseRecord(await env.NOTIFY.get(key.name));
      rows.push([csvField(email), csvField(ts), csvField(token)].join(","));
    }
    cursor = listed.list_complete ? undefined : listed.cursor;
  } while (cursor);

  return new Response(rows.join("\n") + "\n", {
    headers: {
      "content-type": "text/csv; charset=utf-8",
      "content-disposition": 'attachment; filename="notify-list.csv"',
    },
  });
}
