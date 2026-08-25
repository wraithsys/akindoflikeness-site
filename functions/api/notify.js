/* Opt-in mailing list endpoint (Cloudflare Pages Function).
 * POST {email} -> stored in the KV namespace `akol-notify`.
 * No cadence, no third party: emails land in your own KV, you mail the list
 * only when a build ships.
 *
 * The namespace is named akol-notify, but a Function only ever sees the
 * BINDING name, which in this project is KV_BINDING. This used to read
 * env.NOTIFY only, which is bound to nothing — so every signup returned ok
 * and was thrown away. It now takes either name.
 *
 * It also no longer reports success when there is nowhere to write. A form
 * that says "you're on the list" and discards the address is worse than a
 * form that admits it is broken.
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
    const kv = env.KV_BINDING || env.NOTIFY;
    if (!kv) {
      return json({ ok: false, error: "the list is not reachable right now" }, 500);
    }
    await kv.put("sub:" + email, new Date().toISOString());
    return json({ ok: true });
  } catch {
    return json({ ok: false, error: "something went wrong" }, 400);
  }
}
