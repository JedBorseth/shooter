#!/usr/bin/env bash
# Runs a GPU measurement with the NVIDIA card held in its performance state and
# the command at a raised CPU priority, so GPU times compare across runs on a
# busy machine:
#
#   tools/steady.sh tools/benchmark.sh --out after.json
#
# On a machine shared with other GPU work, hold its lock around it, for
# example `flock ~/.cache/shooter-fidelity/gpu.lock tools/steady.sh ...`.
#
# Why: GPU time is read from the renderer's timestamps, so it scales with the
# card's clock. On a light preset (Low), and whenever other processes slow the
# frame rate, the card idles between frames and the driver drops its clock
# (999 to 1923 MHz within one run on the reference card), and the readings
# grow with it: Low read 4.7 to 5.8 ms on maps that cost 3.4 to 4.0. GeForce
# cards of this age cannot have their clock locked (`nvidia-smi -lgc` is not
# supported), so this starts a headless X server on the card's own driver
# for the length of the command, sets the driver's PowerMizer "prefer maximum
# performance" mode through it (the clock never drops below base, 1569 MHz),
# runs the command on that display (see below), and stops the server after.
# Runs then repeat to within about 0.03 ms per view, at the clocks a
# player's card runs at.
#
# Needs passwordless sudo, Xorg with the NVIDIA driver, and nvidia-settings;
# without them it runs the command unchanged and says so. STEADY_GPU_BUS sets
# the card's PCI bus id for Xorg (default: the first card nvidia-smi lists).
set -euo pipefail
if [[ $# -eq 0 ]]; then
	echo "usage: tools/steady.sh <command> [args...]" >&2
	exit 2
fi
if ! command -v nvidia-smi >/dev/null || ! command -v nvidia-settings >/dev/null \
	|| ! command -v Xorg >/dev/null || ! sudo -n true 2>/dev/null; then
	echo "steady: needs nvidia-smi, nvidia-settings, Xorg, and passwordless sudo; running unpinned" >&2
	exec "$@"
fi
bus="${STEADY_GPU_BUS:-}"
if [[ -z "$bus" ]]; then
	# 00000000:01:00.0 -> PCI:1:0:0 (decimal fields).
	pci="$(nvidia-smi -i 0 --query-gpu=pci.bus_id --format=csv,noheader)"
	IFS=':.' read -r _ b d f <<<"$pci"
	bus="PCI:$((16#$b)):$((16#$d)):$((16#$f))"
fi
work="$(mktemp -d "${XDG_RUNTIME_DIR:-/tmp}/steady.XXXXXX")"
cat >"$work/xorg.conf" <<EOF
Section "ServerFlags"
    Option "AutoAddGPU" "false"
    Option "AutoAddDevices" "false"
EndSection
Section "Device"
    Identifier "Card"
    Driver "nvidia"
    BusID "$bus"
    Option "AllowEmptyInitialConfiguration" "True"
EndSection
Section "Screen"
    Identifier "Screen"
    Device "Card"
    DefaultDepth 24
    SubSection "Display"
        Depth 24
        Virtual 1920 1080
    EndSubSection
EndSection
EOF
# A free display number.
display=60
while [[ -e "/tmp/.X11-unix/X$display" || -e "/tmp/.X$display-lock" ]]; do
	display=$((display + 1))
done
# shellcheck disable=SC2024 # the logs are ours to write, not root's
sudo -n Xorg ":$display" -config "$work/xorg.conf" -noreset -nolisten tcp \
	-logfile "$work/xorg.log" >"$work/xorg.out" 2>&1 &
xorg=$!
cleanup() {
	sudo -n kill "$xorg" 2>/dev/null || true
	wait "$xorg" 2>/dev/null || true
	rm -rf "$work"
}
trap cleanup EXIT
for _ in $(seq 50); do
	[[ -e "/tmp/.X11-unix/X$display" ]] && break
	sleep 0.2
done
if ! DISPLAY=":$display" nvidia-settings -a '[gpu:0]/GPUPowerMizerMode=1' >/dev/null 2>&1; then
	echo "steady: no X server on the card (log below); running unpinned" >&2
	tail -5 "$work/xorg.log" >&2 || true
	"$@"
	exit
fi
echo "steady: card held in its performance state on display :$display" >&2
# Render on the card's own display, not Xvfb: frames are presented on the
# GPU instead of copied through the CPU (about 8 ms a frame at 1080p), so the
# frame rate is not capped near 60 to 100 fps and the card stays busy enough to
# boost as it would for a player. tools/benchmark.sh and tools/cpu_bench.sh
# use DISPLAY when it is set; commands that call xvfb-run themselves still get
# Xvfb, and only the held performance state.
export DISPLAY=":$display" STEADY_DISPLAY=":$display"
# sudo resets the environment: hand the caller's back to the command.
mapfile -d '' vars < <(env -0)
sudo -n nice -n -15 runuser -u "$(id -un)" -- env "${vars[@]}" "$@"
