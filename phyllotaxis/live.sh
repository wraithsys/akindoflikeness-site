#!/usr/bin/env bash
# One command to start the chat relay, from anywhere. See LIVE.md.
#
#   ./live.sh twitch <channel>
#   ./live.sh youtube <url-or-video-id>
#   ./live.sh test                        # type `name: message` lines yourself
set -euo pipefail
cd "$(dirname "$0")"

BIN=target/release/phyllotaxis-live
[ -x "$BIN" ] || cargo build --release -p phyllotaxis-live

case "${1:-}" in
  twitch)
    exec "$BIN" twitch "${2:?usage: ./live.sh twitch <channel>}"
    ;;
  youtube)
    ./yt-chat.py "${2:?usage: ./live.sh youtube <url-or-video-id>}" | "$BIN" stdin
    ;;
  test)
    exec "$BIN" stdin
    ;;
  *)
    echo "usage: ./live.sh twitch <channel> | youtube <url-or-id> | test" >&2
    exit 2
    ;;
esac
