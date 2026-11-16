#!/bin/sh
# Idempotent cleanup, also used by systemd after an interrupted game process.
set -eu
state_file="$HOME/.cache/shooter-nuketown/paused-services"
[ -f "$state_file" ] || exit 0
if grep -qx tts "$state_file"; then systemctl --user start qwen3-tts.service; fi
if grep -qx music "$state_file"; then sudo -n docker start jedflix-music-ai-1 >/dev/null; fi
rm -f "$state_file"
