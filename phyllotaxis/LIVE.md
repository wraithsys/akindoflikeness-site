# The streaming rig

A constant stream of PHYLLOTAXIS playing itself, where the chat plays it too:
a viewer shapes a patch on the public page, presses SHARE, pastes the URL into
the stream chat, and the stream takes that patch — with their name on screen
next to it.

Two pieces, both in this repo:

- **`?live` on the instrument page** — `/instruments/phyllotaxis/?live` polls
  `http://127.0.0.1:7341/patch` every 2 s and applies whatever patch the relay
  holds, crediting the sender in the header. Without the query param none of
  that code runs; the public page never makes a request.
- **`phyllotaxis-live`** (`crates/phyllotaxis-live`) — reads the stream chat,
  fishes out anything that looks like a shared patch, and re-serves the current
  one on loopback for the page to poll.

## Running it

One terminal, from this directory (`live.sh` builds the relay if needed):

    ./live.sh twitch <channel>          # anonymous, no account or token
    ./live.sh youtube <url-or-video-id> # via yt-chat.py, see below
    ./live.sh test                      # type `name: message` lines yourself

YouTube's official API can't sustain 24/7 chat polling on default quota, and
chat-downloader (the usual answer) no longer parses current YouTube — its
last release predates a site change. `yt-chat.py` uses yt-dlp instead
(`pip install yt-dlp`, and keep it updated: tracking YouTube is its whole
job), tailing the live-chat file yt-dlp records into clean `user: message`
lines. Anything else that can print such lines works via `... | 
phyllotaxis-live stdin`.

The picture: OBS's **Browser Source** is a built-in Chromium, so the page
needs no separate browser window and no window capture. Add a Browser source
with the URL below, canvas-sized (e.g. 1920×1080), tick **Control audio via
OBS** — it autoplays without a click, so a rebooted rig does not sit silent
behind the Begin button:

    https://akindoflikeness.net/instruments/phyllotaxis/?live

(If a real browser window is ever preferred, launch Chromium with
`--autoplay-policy=no-user-gesture-required` and the same URL; without the
flag, click Begin once.)

Under `?live` the page trades the reading layout for a broadcast frame: the
stage takes the height of the 1080p canvas and the racks stand beside it as a
rail — the sliders moving under a viewer's patch are part of the show. The
canvas renders 1:1 with the browser source's pixels, so nothing is upscaled
into softness and then paid for in bitrate.

**Encoder settings the page is tuned for:** 1080p at 60 fps, 12000 kbps
(YouTube's recommended rate for 1080p60), keyframe interval 2 s. In the
browser source properties tick nothing unusual — but if the stream looks
30fps, set the source's custom FPS to 60 or raise OBS's canvas FPS; browser
sources follow the canvas. The visual is encoder-friendly by construction:
true blacks, a feedback trail that gives the codec temporal coherence, and
marks floored at a 720p-equivalent pixel so nothing shimmers at sub-pixel
size.

## What the stream does between patches

Two things happen to every patch the relay hands over, both seeded from the
patch itself (the same patch replants the same plant; sending it twice is
two plantings):

- **The plant reseeds.** New divergence angle — drawn from the fifteen
  angles Billy chose on the workbench (117–170°, the golden angle never
  among them, per the taste ledger's law 4) — new growth rate, spin, trail,
  and a slow curvature breath borrowed from the starfield presets. The field
  collapses to a sprout and reassembles over 8–34 s while the old plant
  erodes in its own trail.
- **The space reseeds.** The plant is no longer only itself relative to
  itself: around it hangs the hostile-visualiser starfield — drift,
  parallax, curvature, shear, edge shimmer, and lensing with the plant's
  actual rim as the subject, so a sprout barely dents the sky and a grown
  plant displaces it. Each planting draws its own sky from the ranges of
  the eight kept starfield presets, occasionally a bare void, fading in
  over ~5 s. The object also floats — a slow bob and sway, never quite
  still — and the lens follows it.
- **The director reseeds.** FM 1:1 stays the voice (`?live&voice=roster`
  restores the roster if that pin turns out wrong); the patch's entry knob
  is re-routed to choose the director's route — which of ROOT, MIRROR,
  VOICING and PACE move. After honouring the same 30 s floor as the relay,
  the director explores: roots step by the derived scale's own nearest
  fourth, fifth and third (FM 1:1's table found 498 and 702 by itself, so
  its cadences are plagal and authentic by derivation); Mirror steps are the
  engine's negative-harmony modal interchange; voicings are composed on the
  tuning channel — rotations of the derived scale are its modes, subsets its
  own triads and open fifths, shown after the dot in the `#kind` readout.
  Everything moves through the page's existing inputs, so the director can
  reach nothing a slider can't, and the address bar stays a truthful patch.

Stream that scene, and put something like this in the description:

> This instrument is playing itself. To play it yourself: open
> https://akindoflikeness.net/instruments/phyllotaxis/ — shape a patch, press
> SHARE, paste the link into the chat. The stream will take your patch and put
> your name on it.

## Policy, all of it

- **Latest wins, but a patch holds the floor** for `--hold` seconds
  (default 30). While the floor is held, newer patches replace the waiting one
  rather than queueing — the stream morphs, it doesn't thrash, and nobody's
  patch plays long after they left.
- **Bare fragments count.** YouTube chat likes to eat links, so
  `0:2,1:6.150,18:82.5,...` pasted without the URL is accepted anywhere in a
  message. Four `id:value` pairs minimum, so "see you at 1:30" is not a patch.
- **Chat can't reach anything a slider can't.** The relay never interprets
  values; the page applies patches through its own inputs, and the browser
  clamps every value to the slider's range. Usernames are stripped to
  `[A-Za-z0-9_]`, 25 chars, before they touch the page.
- `--port` moves the loopback port (open the page with `?live=<port>` to
  match). The relay binds 127.0.0.1 only.

## Restarting

Everything is stateless. If the relay dies, restart it — the page keeps
polling and picks it back up. If the page dies, reopen it — the first poll
returns the current patch, so the stream comes back in the state it was in,
not at the defaults.
