#!/usr/bin/env python3
"""YouTube live chat as `user: message` lines, for `phyllotaxis-live stdin`.

    ./yt-chat.py <url-or-video-id> | phyllotaxis-live stdin

chat-downloader stopped parsing current YouTube and its last release predates
the change; yt-dlp keeps up because it never stops being maintained. yt-dlp
can record a live stream's chat as it happens (--write-subs live_chat), but
only into a growing file — so this script runs yt-dlp into a temp dir, tails
that file, and prints one clean line per message. Latency is one chat
fragment, a few seconds, which the relay's --hold floor dwarfs anyway.
"""

import glob
import json
import os
import subprocess
import sys
import tempfile
import time


def emit(line):
    try:
        d = json.loads(line)
    except ValueError:
        return
    # Live chat is recorded in replay framing; take bare actions too in case
    # a yt-dlp version drops the wrapper.
    acts = (
        d.get("replayChatItemAction", {}).get("actions")
        or d.get("actions")
        or ([d] if "addChatItemAction" in d else [])
    )
    for act in acts:
        r = act.get("addChatItemAction", {}).get("item", {}).get(
            "liveChatTextMessageRenderer"
        )
        if not r:
            continue
        name = r.get("authorName", {}).get("simpleText", "chat").lstrip("@")
        text = "".join(
            run.get("text", "") for run in r.get("message", {}).get("runs", [])
        )
        if text:
            print(f"{name}: {text}", flush=True)


def chat_file(base):
    # `.part` while yt-dlp runs, renamed on finish; the in-progress fragment
    # files (`…-FragN`) are partial JSON and must not be read.
    for path in (base + ".live_chat.json", base + ".live_chat.json.part"):
        if os.path.exists(path):
            return path
    return None


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__.strip())
    url = sys.argv[1]
    if "youtube.com" not in url and "youtu.be" not in url:
        url = "https://www.youtube.com/watch?v=" + url

    with tempfile.TemporaryDirectory(prefix="yt-chat-") as d:
        base = os.path.join(d, "chat")
        p = subprocess.Popen(
            ["yt-dlp", "--quiet", "--no-warnings", "--skip-download",
             "--write-subs", "--sub-langs", "live_chat", "-o", base, url],
        )
        offset = 0
        tail = b""
        try:
            while True:
                path = chat_file(base)
                if path:
                    with open(path, "rb") as f:
                        f.seek(offset)
                        chunk = f.read()
                    offset += len(chunk)
                    tail += chunk
                    *lines, tail = tail.split(b"\n")
                    for line in lines:
                        emit(line)
                if p.poll() is not None:
                    # Stream over, or yt-dlp fell over; either way the pipe
                    # ends and whoever ran us decides about restarting.
                    if tail:
                        emit(tail)
                    sys.exit(p.returncode)
                time.sleep(2)
        finally:
            if p.poll() is None:
                p.terminate()


if __name__ == "__main__":
    main()
