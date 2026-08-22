/* Is the stream on? (Cloudflare Pages Function.)
 * GET -> { live, url, title? }
 *
 * No YouTube API key: /@handle/live is a plain page whose <link rel=canonical>
 * points at a watch URL only while the channel is actually live — offline it
 * canonicalises back to the channel. Reading that link is the whole check.
 * The Data API was rejected for the same reason it lost in LIVE.md: default
 * quota cannot sustain a public page polling it.
 *
 * Cached in the edge cache so a page full of viewers costs YouTube (and us)
 * one fetch a minute, not one per visitor. The cache strategy is
 * stale-while-revalidate, done by hand because the Cache API does not honour
 * that directive itself:
 *
 *   - the entry is stored for STALE_S (an hour), stamped with the time it
 *     was fetched (`x-fetched-at`);
 *   - inside FRESH_S (60s) it is returned as-is;
 *   - past FRESH_S it is still returned immediately, and a refresh runs in
 *     waitUntil so the *next* viewer gets the new answer.
 *
 * Why: zone analytics for 2026-08-19..22 showed this endpoint answering 504
 * whenever YouTube stalled — the runtime's fetch timeout is far longer than
 * a page is willing to wait, and the visitor who arrived first after the
 * 60s expiry ate the whole stall. The page JS swallows the error (the live
 * row simply never appears), so the cost was silent: an "offline" answer on
 * a night the stream was on. Now the fetch is cut at FETCH_TIMEOUT_MS and
 * the last good answer is what a viewer sees while the refresh happens
 * behind them.
 */
const CHANNEL = "https://www.youtube.com/@akindoflikeness";
const FRESH_S = 60;          // answer is trusted without asking YouTube again
const STALE_S = 3600;        // answer is still served, but a refresh is kicked off
const FETCH_TIMEOUT_MS = 5000;

const json = (o, fetchedAt) =>
  new Response(JSON.stringify(o), {
    headers: {
      "content-type": "application/json",
      // Browsers: a minute. The edge copy's own lifetime is STALE_S, set on
      // the *stored* copy by store() below, not on this one.
      "cache-control": `public, max-age=${FRESH_S}`,
      "x-fetched-at": String(fetchedAt),
    },
  });

// The stored copy carries a long max-age so the Cache API keeps it for the
// whole stale window; the copy handed to the browser keeps the short one.
const store = (cache, key, res) => {
  const kept = new Response(res.body, res);
  kept.headers.set("cache-control", `public, max-age=${STALE_S}`);
  return cache.put(key, kept);
};

async function askYouTube() {
  let out = { live: false, url: CHANNEL };
  try {
    const r = await fetch(CHANNEL + "/live", {
      redirect: "follow",
      signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
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
    return { out, ok: true };
  } catch {
    // YouTube unreachable, or slower than FETCH_TIMEOUT_MS: report offline
    // rather than erroring — the section on the page degrades to the channel
    // link either way. `ok: false` tells the caller not to overwrite a good
    // cached answer with this guess.
    return { out, ok: false };
  }
}

export async function onRequestGet({ request, waitUntil }) {
  const cache = caches.default;
  // The cache key is this endpoint, not the incoming URL: query strings must
  // not fan out into separate uncached fetches of YouTube.
  const key = new Request(new URL("/api/live", request.url).toString());
  const hit = await cache.match(key);
  const now = Date.now();

  if (hit) {
    const fetchedAt = Number(hit.headers.get("x-fetched-at")) || 0;
    if ((now - fetchedAt) / 1000 > FRESH_S) {
      // Stale: hand back the last good answer now, refresh behind the viewer.
      waitUntil(
        askYouTube().then(({ out, ok }) => {
          if (ok) return store(cache, key, json(out, Date.now()));
        }),
      );
    }
    return json(JSON.parse(await hit.text()), fetchedAt);
  }

  // Nothing cached: this viewer waits, but never longer than FETCH_TIMEOUT_MS.
  const { out, ok } = await askYouTube();
  const res = json(out, now);
  // A failed probe is not worth remembering for an hour. It is stored with
  // the short max-age only, so a stalled YouTube is asked once a minute,
  // not once per visitor.
  waitUntil(ok ? store(cache, key, res.clone()) : cache.put(key, res.clone()));
  return res;
}
