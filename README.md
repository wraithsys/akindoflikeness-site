# akindoflikeness.net

The AKOL landing page, the BYPO page, and the place web instruments get served
from. Static, no build step in this repo.

Deployed on Cloudflare Pages, connected to this repo: **every push to `main`
redeploys the live site.** Custom domain `akindoflikeness.net`.

## Layout

```
index.html            landing page          /
bypo.html             BYPO product page     /bypo
404.html              not-found page
favicon.ico           served from the root so browsers that probe for it hit
assets/fonts/         the three faces, + OFL.txt and NOTICE.md
assets/img/           wordmark, nav mark, hero poster, dither tile, icons, OG card
covers/               album art
instruments/          web instruments — cross-origin isolated, see its README
functions/api/        Pages Functions (the opt-in mailing list endpoint)
_headers              cache policy + the COOP/COEP scope
_routes.json          keeps static assets off the Functions runtime
phyllotaxis/          Rust workspace for the next instrument (source, not a page)
                      NOTE: this repo is PUBLIC and this directory is
                      inside the deploy root. See "phyllotaxis/ is source,
                      and it is here on purpose" below before adding to it.
```

## index.html is a build artefact

The source of truth for the landing page is `perflab/site/akol-net.template.html`
in the main dev tree, which is **not in this repo**. `index.html` here is the
built, domain-corrected output.

**So the template has to be kept in step with this file, or the next build
undoes what is here.** What a build would currently overwrite:

- the document head — doctype, `lang`, charset, viewport, description,
  canonical, OG/Twitter tags, icon links, font preloads;
- `<link>`/`src`/`url()` pointing at `/assets/…` instead of `data:` URIs;
- the `<h1>` wrapping the hero wordmark;
- `role="img"` on the two canvases;
- the footer links (both used to point at `#instruments`).

The files under `assets/` survive a rebuild either way. `_headers`,
`_routes.json`, `404.html`, `favicon.ico` and everything under `instruments/`
are not generated and are safe.

## Assets are files now, not data URIs

They used to be base64 inside the HTML. The cost of that was not just page
weight — it was that the browser could not cache any of it, and every edit to a
paragraph rewrote every byte.

| | before | after |
|---|---|---|
| `index.html` | 413 kB | 27 kB |
| `bypo.html` | 88 kB | 11 kB |
| first visit to `/` | 413 kB | ~95 kB |
| second visit | 413 kB | ~27 kB |
| `/` then `/bypo` | 501 kB | ~106 kB |

Two of the un-inlined assets are now fetched by nobody:

- **`pixeloid-mono.ttf` (103 kB)** is asked for by exactly one CSS rule, `h1`,
  which sets `"Alkhemikal", "Pixeloid Mono"` — and no `Alkhemikal` face is
  defined while the only `<h1>` holds an image. A webfont with no glyphs to
  render is never downloaded, so un-inlining it took 103 kB off every visit
  without deleting anything or changing the design.
- **`hero-poster.png` (117 kB)** is the `<noscript>` fallback. Inlined, it was
  paid for by everyone; as a file, `<noscript>` content is not parsed when
  scripting is on, so it is fetched only by the visitors it is for.

Fonts stay TTF rather than WOFF2 — Pages compresses on the wire and brotli'd
pixel TTF lands within a few kB of the equivalent WOFF2. See
`assets/fonts/NOTICE.md` for licensing, which un-inlining does not change:
shipping a font is redistribution either way.

## Caching

`_headers` has the reasoning inline, including two Cloudflare `_headers` quirks
that fail silently and are easy to get wrong. The short version: `/assets/*` is
immutable for a year and is therefore **rename-to-change**; `/covers/*` gets a
month; HTML gets Pages' own `max-age=0, must-revalidate`, so a deploy is live
immediately.

## Web instruments

`/instruments/*` is served cross-origin isolated (COOP/COEP) so
`SharedArrayBuffer` is available to WASM instruments. It is scoped to that
subtree on purpose — site-wide isolation would blank the YouTube embed on
`/bypo`. See `instruments/README.md`.

## The mailing list

`functions/api/notify.js` stores opt-ins in a KV namespace bound as `NOTIFY`
(Pages dashboard → Settings → Functions → KV bindings). Until it is bound the
form returns ok and stores nothing.

## Open

- Horizon (the wordmark) still has unconfirmed terms — see the note in
  `index.html` and `assets/fonts/NOTICE.md`.
- The `--field` grey (`#424242`) on black is 2.1:1. It sets the footer, the
  colophon, the cover metadata and the form's status line. It is a deliberate
  part of the chrome ladder, so it has been left alone, but it is below WCAG AA
  and those strings are hard to read.


## phyllotaxis/ is source, and it is here on purpose

**This repository is public**, so the `phyllotaxis/` workspace — `DESIGN.md` and
all of its source — is readable by anyone. That was raised as a problem and
Billy's call is that it is not one: *"keep it where it is, it's not causing any
problems."* `fibonacci-synth` is public and MIT already, so this is the same
posture, not a new one.

Two mechanics that follow from it, both handled:

- Cloudflare Pages publishes the repository root, so once this branch reaches
  `main` the workspace is also served at `akindoflikeness.net/phyllotaxis/`.
  `robots.txt` disallows it — source directories are not pages and should not be
  in an index — but they remain readable, which is fine.
- `phyllotaxis/renders/` is gitignored. The WAVs are large, regenerate exactly,
  and would otherwise be published too.

## Titles are not search terms, deliberately

`AKOL — art / tools / systems` and `BYPO — blow your phase off` were briefly
rewritten to carry search keywords — "a free FM drone synth" and similar. Billy
reverted it: *"let's not sloppify the site and downsell our own vision and
innovation."*

He is right and the reasoning is worth keeping. Filing BYPO under "free FM drone
synth" makes it findable by people looking for the category it exists to not be
in, and the ones who would actually want it cannot search for a thing that has
no name yet. The structured data below carries the descriptive language for
machines; the visible copy does not have to repeat it for humans.

Keep the plumbing, leave the voice alone.

## activate.akindoflikeness.net

Not a page, and it should not be linked from the footer. It is the activation
server for **Slate Shell** (`wraithsys/slate_shell`) — an Axum/SQLite service
behind a Cloudflare tunnel that checks license keys against a machine
fingerprint. Hardcoded at `crates/slate_shell/src/slates/unlock.rs:10`.

The footer link that pointed at it was a placeholder aimed at `#instruments`.
When Slate Shell ships, the thing to link is the **installer download**, not the
activation host.
