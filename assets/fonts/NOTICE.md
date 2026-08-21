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

Pixeloid Mono — Copyright (c) 2020-2022 GGBotNet (https://ggbot.net/fonts/),
Reserved Font Name "Pixeloid". The OFL's Reserved Font Name clause means a
*modified* copy may not be called "Pixeloid"; shipping it unmodified under its
own name, with this notice, is exactly what the licence asks for.

CC0 faces carry no conditions. They are credited because the colophon credits
them, not because they must be.

Panoptic Monospace — by Josiah Bishop; *"Panoptic Monospace has been released
into the Public Domain"* (1001fonts.com licence page, checked 2026-08-21;
Billy's download, same day). It is the live-rendered wordmark face — the first
face on this site allowed to *be* a font file in the wordmark register, because
public domain has no redistribution terms to trip over.

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
