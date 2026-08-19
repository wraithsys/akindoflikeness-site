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
                      NOTE: this repo is PUBLIC. See "phyllotaxis is in the
                      wrong repo" below before pushing anything else here.
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

- `phyllotaxis/` is inside the deploy root, so its source and `DESIGN.md` are
  publicly readable at `akindoflikeness.net/phyllotaxis/…`. If that is not
  wanted, the workspace needs to move out of the published directory, or the
  Pages project needs a build step with an output directory.
- Horizon (the wordmark) still has unconfirmed terms — see the note in
  `index.html` and `assets/fonts/NOTICE.md`.
- The `--field` grey (`#424242`) on black is 2.1:1. It sets the footer, the
  colophon, the cover metadata and the form's status line. It is a deliberate
  part of the chrome ladder, so it has been left alone, but it is below WCAG AA
  and those strings are hard to read.


## phyllotaxis is in the wrong repo

**This repository is public.** The `phyllotaxis/` workspace was put here because
repository creation is blocked for the automation account, and that was a
mistake: its `DESIGN.md` and full source are readable by anyone on GitHub today,
on the `claude/feedback-request-b0u1rw` branch, regardless of what Cloudflare
does or does not publish.

It needs to move to a private repository of its own. Until it does, do not push
further phyllotaxis work here. The branch history can be rewritten to drop it
once it has somewhere to go — nothing depends on that branch.

Separately, and only relevant after the move: Cloudflare Pages publishes the
repository root, so any source directory left here would also be served at
`akindoflikeness.net/<dir>/`. `robots.txt` disallows `/phyllotaxis/` as a
stopgap, which keeps it out of search results but does not make it unreadable.

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
