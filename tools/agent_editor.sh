#!/usr/bin/env bash
# Starts, stops, or checks a Godot editor that agents can drive through the
# editor MCP bridge (godot/addons/godot_ai_mcp + tools/godot-mcp; see
# docs/editor-mcp.md):
#
#   tools/agent_editor.sh [start]   # or `just agent-editor`
#   tools/agent_editor.sh status
#   tools/agent_editor.sh stop
#
# The editor needs a display. It uses $DISPLAY when set; otherwise a desktop
# this user already has running (a remote desktop's Xorg, as on a GPU server);
# otherwise a virtual one (xvfb-run, with the OpenGL renderer, since a virtual
# display has no GPU: slow, but it works). GODOT_MCP_PORT (default 6505) picks
# the plugin's port, so each checkout can run its own editor; the bridge reads
# the same variable. The editor keeps running after this script exits.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
port="${GODOT_MCP_PORT:-6505}"
godot="${GODOT:-godot}"
state="${XDG_STATE_HOME:-$HOME/.local/state}/shooter"
pid_file="$state/editor-$port.pid"
log="$state/editor-$port.log"
mkdir -p "$state"

listening() {
	(exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null
}

running_pid() {
	[[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" 2>/dev/null && cat "$pid_file"
}

# Points DISPLAY and XAUTHORITY at a desktop this user already runs (for
# example an xrdp session's Xorg), if there is one. Prints nothing otherwise.
use_running_desktop() {
	local xorg display auth
	xorg="$(pgrep -u "$(id -u)" -x Xorg | head -n 1 || true)"
	[[ -n "$xorg" ]] || return 1
	display="$(tr '\0' ' ' <"/proc/$xorg/cmdline" | grep -oE ' :[0-9]+' | head -n 1 | tr -d ' ')"
	[[ -n "$display" ]] || return 1
	auth="$state/Xauthority"
	: >"$auth"
	chmod 600 "$auth"
	if [[ -r "/proc/$xorg/cwd/.Xauthority" ]]; then
		# Any-host entries, so the cookie works whatever this shell's hostname.
		xauth -f "/proc/$xorg/cwd/.Xauthority" nlist | sed 's/^..../ffff/' | xauth -f "$auth" nmerge -
	fi
	local socket="/tmp/.X11-unix/X${display#:}"
	if [[ ! -e "$socket" ]]; then
		mkdir -p /tmp/.X11-unix
		ln -sfn "/proc/$xorg/root$socket" "$socket"
	fi
	export DISPLAY="$display" XAUTHORITY="$auth"
}

start() {
	if listening; then
		echo "An editor is already serving agents on port $port (log: $log)."
		return 0
	fi
	(cd "$repo/rust" && cargo build --quiet)
	local cmd=("$godot" --editor --path "$repo/godot")
	local where
	if [[ -n "${DISPLAY:-}" ]]; then
		where="display $DISPLAY"
	elif use_running_desktop; then
		where="running desktop $DISPLAY"
	elif command -v xvfb-run >/dev/null; then
		cmd=(xvfb-run -a -s "-screen 0 1920x1080x24" "${cmd[@]}" --rendering-driver opengl3)
		where="a virtual display (software OpenGL)"
	else
		echo "No display: set DISPLAY, start a desktop session, or install xvfb (xvfb-run)." >&2
		return 1
	fi
	rm -f "$pid_file"
	# Its own session, so it outlives this shell; the shell records its pid,
	# which is also its process group's id, before becoming the editor.
	GODOT_MCP_PORT="$port" setsid sh -c 'echo $$ >"$0"; exec "$@"' "$pid_file" "${cmd[@]}" \
		>"$log" 2>&1 </dev/null &
	for _ in $(seq 1 50); do
		[[ -s "$pid_file" ]] && break
		sleep 0.1
	done
	echo "Starting the editor on $where; waiting for the MCP plugin on port $port..."
	for _ in $(seq 1 180); do
		if listening; then
			echo "Ready: agents can connect (the bridge uses GODOT_MCP_PORT=$port). Log: $log"
			return 0
		fi
		if ! kill -0 "$(cat "$pid_file")" 2>/dev/null; then
			echo "The editor exited. Last lines of $log:" >&2
			tail -n 20 "$log" >&2
			return 1
		fi
		sleep 1
	done
	echo "The editor started but the plugin is not listening after 3 minutes; see $log" >&2
	return 1
}

stop() {
	local pid
	if pid="$(running_pid)"; then
		# setsid made the editor its own process group; end the group (xvfb-run too).
		kill -- "-$pid" 2>/dev/null || kill "$pid"
		rm -f "$pid_file"
		echo "Stopped the editor on port $port."
	else
		echo "No editor started by this script on port $port."
	fi
}

status() {
	if listening; then
		echo "Port $port: an editor is serving agents. Log: $log"
	else
		echo "Port $port: nothing listening."
	fi
}

case "${1:-start}" in
start) start ;;
stop) stop ;;
status) status ;;
*)
	echo "usage: $0 [start|stop|status]" >&2
	exit 2
	;;
esac
