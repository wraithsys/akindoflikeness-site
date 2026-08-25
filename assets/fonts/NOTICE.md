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
| `alkhemikal.ttf` | Alkhemikal | CC BY 4.0 — jeti, fontenddev.com |
| `inter-latin-400.woff2` | Inter | SIL Open Font License 1.1 — see `OFL.txt` |

Pixeloid Mono — Copyright (c) 2020-2022 GGBotNet (https://ggbot.net/fonts/),
Reserved Font Name "Pixeloid". The OFL's Reserved Font Name clause means a
*modified* copy may not be called "Pixeloid"; shipping it unmodified under its
own name, with this notice, is exactly what the licence asks for.

Alkhemikal is **CC BY 4.0**, which is the only face here with a live
condition attached: it requires attribution wherever it is used. That
attribution is the first clause of the colophon line at the foot of `bypo.html`
and `index.html`. Deleting that line breaks the licence. It is display only —
the "Wraith" lockup — never body, per the dev tree's font rules.

Inter is OFL like Pixeloid Mono, no Reserved Font Name issue, shipped
unmodified as `inter-latin-400.woff2`. It is used at 8-9px for the colophon
only, where a bitmap face at that size would be unreadable.

CC0 faces carry no conditions. They are credited because the colophon credits
them, not because they must be.

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
