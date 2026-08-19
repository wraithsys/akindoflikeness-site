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

Build once:

    cargo build --release -p phyllotaxis-live

Start the relay against the chat:

    # Twitch — anonymous, no account or token, reconnects on its own
    ./target/release/phyllotaxis-live twitch <channel>

    # YouTube — the official API can't sustain 24/7 chat polling on default
    # quota, so pipe chat-downloader (pip install chat-downloader) instead:
    chat_downloader "https://www.youtube.com/watch?v=<video-id>" \
      | ./target/release/phyllotaxis-live stdin

    # Anything else that can print `user: message` lines also works via stdin.

Open the instrument in the browser OBS captures:

    chromium --autoplay-policy=no-user-gesture-required \
      "https://akindoflikeness.net/instruments/phyllotaxis/?live"

With that flag the page starts itself — a rebooted rig does not sit silent
behind a Begin button. Without it, click Begin once; live mode works the same.

Point OBS at that window, stream it, and put something like this in the
description:

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
