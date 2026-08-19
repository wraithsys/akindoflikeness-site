# /instruments — web instruments

Things that run *in the browser* live here. Not product pages: `/bypo` is a page
about a Windows binary and stays at the site root, where it can keep its YouTube
embed. This directory is for instruments the visitor actually plays.

One directory per instrument:

```
instruments/<name>/
  index.html          the page          — served at /instruments/<name>/
  build/              the built bundle  — .wasm, the worklet, the glue JS
```

## What the directory gets for free

`_headers` at the repo root applies `/instruments/*`:

```
Cross-Origin-Opener-Policy:   same-origin
Cross-Origin-Embedder-Policy: require-corp
Cross-Origin-Resource-Policy: same-origin
```

so every page here is **cross-origin isolated** and `SharedArrayBuffer` is
available. Check it at runtime with `crossOriginIsolated === true` before
allocating the ring buffer, and keep the transferable fallback for the case
where it is false — a stray header change should degrade the visualiser, not
break the instrument.

`.wasm` is already served as `application/wasm` by Pages, so
`WebAssembly.compileStreaming(fetch(...))` works with no configuration.

## What it costs

Isolation is not free: **no third-party embeds**. A YouTube iframe, a Bandcamp
player, an image hotlinked from anywhere else — all of them are blocked here
unless they send CORP or CORS headers, and most do not. If an instrument page
needs a demo video, self-host it or link out to `/bypo`-style page instead.

## Adding a bundle

The bundle is the one thing that changes every deploy, so it must not live
inside the HTML. Emit it into `build/` with a content hash in the filename
(`phyllotaxis.9f2c41.wasm`), have `index.html` reference the hashed name, and
add one line to `_headers`:

```
/instruments/<name>/build/*
  Cache-Control: public, max-age=31536000, immutable
```

One line per instrument, not a pattern — a `_headers` rule may contain only one
`*`, so `/instruments/*/build/*` silently matches nothing. The root `_headers`
explains the rest of that syntax.

The HTML itself needs no cache rule: Pages defaults to
`max-age=0, must-revalidate`, so a new bundle is picked up on the next request.

## Not here yet

PHYLLOTAXIS. The Rust workspace is in `/phyllotaxis` at the repo root and
`DESIGN.md` is its spec; the public name is still open (DESIGN.md, "Open"), so
the directory here is not named yet.
