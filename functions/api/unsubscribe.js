/* Unsubscribe from the mailing list (Cloudflare Pages Function).
 * GET ?token=<the token from an unsubscribe link> -> removes the KV entry,
 * answers with a plain house-styled HTML page (this is a link a mail client
 * opens in a browser, never fetch()'d by the site's own JS, so JSON would be
 * the wrong shape here).
 *
 * Companion to functions/api/notify.js, which writes the `tok:<token> ->
 * email` index this reads. GDPR/PECR requires a working unsubscribe path
 * for direct marketing (ico.org.uk, cited in research/site-commerce.md §6) —
 * before this file there was none at all.
 */
const page = (status, heading, body) =>
  new Response(
    `<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${heading} — akindoflikeness.net</title>
<meta name="robots" content="noindex">
<meta name="theme-color" content="#000000">
<link rel="icon" href="/favicon.ico" sizes="32x32">
<style>
  /* Same minimal, no-webfont posture as 404.html: a link from a mail client
     should resolve in one request, not wait on a font file. */
  :root{ --black:#000; --rule:#464646; --text:#A0A0A0; --light:#fff; }
  body{ background:var(--black); color:var(--text);
    font-family:ui-monospace,Consolas,monospace; margin:0; min-height:100vh;
    display:grid; place-items:center; line-height:1.5; }
  .box{ text-align:center; padding:32px; display:flex; flex-direction:column; gap:16px; max-width:52ch; }
  .code{ color:var(--light); font-size:22px; letter-spacing:.06em; font-weight:400; margin:0; }
  .hr{ height:1px; background:var(--rule); }
  nav{ display:flex; gap:24px; justify-content:center; flex-wrap:wrap; }
  a{ color:var(--text); text-decoration:none; letter-spacing:.06em; }
  a:hover, a:focus-visible{ color:var(--light); }
  :focus-visible{ outline:1px solid var(--light); outline-offset:2px; }
</style>
<main class="box">
  <h1 class="code">${heading}</h1>
  <div class="hr"></div>
  <p>${body}</p>
  <nav>
    <a href="/">akindoflikeness.net</a>
    <a href="/terms">terms</a>
    <a href="/#contact">contact</a>
  </nav>
</main>`,
    { status, headers: { "content-type": "text/html; charset=utf-8" } },
  );

export async function onRequestGet({ request, env }) {
  const token = new URL(request.url).searchParams.get("token");

  if (!token) {
    return page(400, "MISSING TOKEN", "this link is missing its token — copy the whole address from the email.");
  }

  if (!(env.NOTIFY || env.KV_BINDING)) {
    console.error("unsubscribe: NOTIFY KV is not bound — cannot process", { token });
    return page(503, "NOT CONFIGURED", "the mailing list isn't wired up on this deploy yet. try again later, or email us directly.");
  }

  const email = await (env.NOTIFY || env.KV_BINDING).get("tok:" + token);
  if (!email) {
    // Idempotent, and no enumeration: an unknown or already-used token reads
    // the same as success, since either way the answer to "am I still on the
    // list" is no.
    return page(200, "ALREADY OFF THE LIST", "that link has already been used, or was never on the list. either way, you're not subscribed.");
  }

  await (env.NOTIFY || env.KV_BINDING).delete("tok:" + token);
  await (env.NOTIFY || env.KV_BINDING).delete("sub:" + email);

  return page(200, "YOU'RE OFF THE LIST", "no more mail from this list. re-subscribe any time from the front page.");
}
