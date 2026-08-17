/* Opt-in mailing list endpoint (Cloudflare Pages Function).
 * POST {email} -> stored in the KV namespace bound as NOTIFY.
 * No cadence, no third party: emails land in your own KV, you mail the list
 * only when a build ships. One-time setup: create a KV namespace and bind it
 * to this Pages project as `NOTIFY` (dashboard: Settings > Functions > KV
 * bindings). Until it's bound the form still returns ok but stores nothing.
 */
const json = (o, s = 200) =>
  new Response(JSON.stringify(o), { status: s, headers: { "content-type": "application/json" } });

export async function onRequestPost({ request, env }) {
  try {
    const body = await request.json();
    const email = String(body.email || "").trim().toLowerCase();
    // one honest validation, no over-engineering
    if (email.length > 200 || !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) {
      return json({ ok: false, error: "that doesn't look like an email" }, 400);
    }
    if (env.NOTIFY) {
      await env.NOTIFY.put("sub:" + email, new Date().toISOString());
    }
    return json({ ok: true });
  } catch {
    return json({ ok: false, error: "something went wrong" }, 400);
  }
}
