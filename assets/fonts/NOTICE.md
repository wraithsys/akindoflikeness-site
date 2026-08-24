# Fonts shipped from this site

These faces used to be base64 data URIs inside `index.html` and `bypo.html`.
They are now real files, which means they are *redistributed* the same way they
were before — the licence has to travel with them either way. That is the gate
from the main dev tree's `assets/fonts/NOTICE.md`: **no font ships without its
licence.**

| file | family | licence |
|---|---|---|
| `pixeloid-mono.ttf` | Pixeloid Mono | SIL Open Font License 1.1 — see `OFL.txt` |
| `modern-dos-8x16.ttf` | Modern DOS 8x16 | CC0 1.0 — Jayvee Enaguas (HarvettFox96) |
| `european-teletext.ttf` | European Teletext | CC0 1.0 |
| `panoptic-monospace-bold.otf` | Panoptic Monospace | Public domain — Josiah Bishop |
| `alkhemikal/Alkhemikal.ttf` | Alkhemikal | CC BY 4.0 — jeti (fontenddev.com), see `alkhemikal/LICENSE.txt` |

Pixeloid Mono — Copyright (c) 2020-2022 GGBotNet (https://ggbot.net/fonts/),
Reserved Font Name "Pixeloid". The OFL's Reserved Font Name clause means a
*modified* copy may not be called "Pixeloid"; shipping it unmodified under its
own name, with this notice, is exactly what the licence asks for.

CC0 faces carry no conditions. They are credited because the colophon credits
them, not because they must be.

Panoptic Monospace — by Josiah Bishop; *"Panoptic Monospace has been released
into the Public Domain"* (1001fonts.com licence page, checked 2026-08-21;
Billy's download, same day). It was briefly wired up as a candidate wordmark
face; that `@font-face` rule has since been removed from `index.html` as dead
code (2026-08-24 — nothing in the markup renders live text in it, the wordmark
stays the Horizon PNG artwork below). The file ships unreferenced — harmless,
since a webfont with no matching CSS rule is never fetched — but it is a
candidate for deletion next time this folder gets a pass.

Alkhemikal — by jeti (fontenddev.com), CC BY 4.0, licence quoted in
`alkhemikal/LICENSE.txt` (author publishes terms on their own site rather than
shipping a licence file; same provenance shape as `hostile-visualiser`'s copy,
which is where this copy came from). Added 2026-08-24 to set "Wraith" in the
header lockup — a display face, not body text, matching its role everywhere
else in Billy's tree. Required credit: *"Alkhemikal by jeti (fontenddev.com),
licensed CC BY 4.0."*

## Not here, deliberately

The **Horizon** wordmark is not a font file in this repo and must not become
one. It ships as artwork — `assets/img/akol-mark.png` (nav),
`assets/img/akol-wordmark.png` (hero), and the derived `og-card.png` /
`icon-*.png` / `favicon.ico`. Billy's Canva Pro licence covers *using* the face
to make designs; it does not cover redistributing `Horizon.woff2`. Rendering to
artwork is what keeps that distinction intact.

Horizon's underlying terms are still unconfirmed against the original foundry
(the aggregator sites contradict each other). That was already flagged in
`index.html` before this change and is unchanged by it — except that the mark
now also appears in link previews via `og-card.png`, which is wider circulation
of the same artwork.

## Why TTF and not WOFF2

Cloudflare Pages compresses on the wire, and a brotli-compressed pixel TTF is
within a few kilobytes of the equivalent WOFF2. Converting would mean
re-generating the files and re-checking the hinting on faces whose entire point
is that they are pixel-exact. Not worth it; revisit only if the fonts grow.

## Inter (footer and colophon, 2026-08-23)

| file | family | licence |
|---|---|---|
| `inter-latin-400.woff2` | Inter, regular weight, latin subset | SIL Open Font License 1.1 (c) 2016 The Inter Project Authors, see `OFL-Inter.txt` |

The footer and colophon moved off the bitmap faces to Inter at 9px / 8px
(Billy: "put the footer in a normal font inter or something and make it very
small"). Bitmap faces off their native grid go soft at that size; a vector
face does not. This is the latin-subset regular weight as served by Google
Fonts (unicode-range U+0000-00FF plus general punctuation, which covers the
em dash in the colophon), 23 kB. Inter's Reserved Font Name is "Inter"; the
file ships unmodified under that name, with this notice and its licence.
