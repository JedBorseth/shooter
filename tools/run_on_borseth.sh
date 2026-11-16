#!/bin/sh
# Run the native game on borseth's existing remote desktop.
set -eu
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cache_dir="$HOME/.cache/shooter-nuketown"
mkdir -p "$cache_dir"
if [ -z "${DISPLAY:-}" ]; then
    xorg_pid=$(pgrep -u "$(id -u)" -x Xorg | head -n 1)
    if [ -z "$xorg_pid" ]; then
        echo 'No remote desktop is running. Start a desktop session first.' >&2
        exit 1
    fi
    export DISPLAY=:10
    export XAUTHORITY="$cache_dir/Xauthority"
    socket=/tmp/.X11-unix/X10
    if [ ! -e "$socket" ] || [ -L "$socket" ]; then
        ln -sfn "/proc/$xorg_pid/root/tmp/.X11-unix/X10" "$socket"
    fi
    touch "$XAUTHORITY"
    chmod 600 "$XAUTHORITY"
    xauth -f "/proc/$xorg_pid/cwd/.Xauthority" nlist |
        sed 's/^..../ffff/' | xauth -f "$XAUTHORITY" nmerge -
fi
godot_bin="${SHOOTER_GODOT:-$HOME/.local/share/shooter-nuketown/bin/godot}"
# Reserve the GPU only when explicitly requested. Restore exactly the services
# that were running when this game session started.
exec 9>"$cache_dir/session.lock"
flock -n 9 || exit 0
if [ "${SHOOTER_RESERVE_GPU:-0}" = 1 ]; then
    state_file="$cache_dir/paused-services"
    umask 077
    : > "$state_file"
    trap '"$project_dir/tools/restore_borseth_gpu.sh"' EXIT
    trap 'exit 143' TERM INT HUP
    if systemctl --user is-active --quiet qwen3-tts.service; then
        echo tts >> "$state_file"
        systemctl --user stop qwen3-tts.service
    fi
    if [ "$(sudo -n docker inspect -f '{{.State.Running}}' jedflix-music-ai-1)" = true ]; then
        echo music >> "$state_file"
        sudo -n docker stop jedflix-music-ai-1 >/dev/null
    fi
fi
"$godot_bin" --path "$project_dir/godot" --windowed --resolution 1280x720 \
    --rendering-method "${SHOOTER_RENDERER:-forward_plus}" --max-fps 60 "$@"
