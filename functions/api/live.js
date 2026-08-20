/* Is the stream on? (Cloudflare Pages Function.)
 * GET -> { live, url, title? }
 *
 * No YouTube API key: /@handle/live is a plain page whose <link rel=canonical>
 * points at a watch URL only while the channel is actually live — offline it
 * canonicalises back to the channel. Reading that link is the whole check.
 * The Data API was rejected for the same reason it lost in LIVE.md: default
 * quota cannot sustain a public page polling it.
 *
 * Cached in the edge cache for 60s so a page full of viewers costs YouTube
 * (and us) one fetch a minute, not one per visitor.
 */
const CHANNEL = "https://www.youtube.com/@akindoflikeness";

const json = (o, extra = {}) =>
  new Response(JSON.stringify(o), {
    headers: { "content-type": "application/json", "cache-control": "public, max-age=60", ...extra },
  });

export async function onRequestGet({ request, waitUntil }) {
  const cache = caches.default;
  // The cache key is this endpoint, not the incoming URL: query strings must
  // not fan out into separate uncached fetches of YouTube.
  const key = new Request(new URL("/api/live", request.url).toString());
  const hit = await cache.match(key);
  if (hit) return hit;

  let out = { live: false, url: CHANNEL };
  try {
    const r = await fetch(CHANNEL + "/live", {
      redirect: "follow",
      headers: {
        // A bare fetch() gets served YouTube's no-JS interstitial; a browsery
        // UA gets the page with the canonical link in it.
        "user-agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
        "accept-language": "en",
      },
    });
    const html = await r.text();
    const canonical = html.match(/<link rel="canonical" href="(https:\/\/www\.youtube\.com\/watch\?v=[\w-]{6,})"/);
    if (canonical) {
      const title = html.match(/<title>([^<]*)<\/title>/);
      out = {
        live: true,
        url: canonical[1],
        title: title ? title[1].replace(/ - YouTube$/, "") : undefined,
      };
    }
  } catch {
    // YouTube unreachable: report offline rather than erroring — the section
    // on the page degrades to the channel link either way.
  }

  const res = json(out);
  waitUntil(cache.put(key, res.clone()));
  return res;
}
