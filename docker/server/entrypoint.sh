#!/bin/sh
# Starts the dedicated server with options from SHOOTER_* environment
# variables, then any arguments given to `docker run` (they win, because the
# server reads the last value of a repeated option). `--health` instead asks
# the running server's query port whether it answers (the image's health check):
# it sends a query (rust/sim/src/net/query.rs: magic, version 2, kind 1, a
# nonce, padded to 160 bytes) and waits three seconds for a reply that starts
# with the magic.
set -eu

port=${SHOOTER_PORT:-27015}
query_port=${SHOOTER_QUERY_PORT:-$((port + 1))}

if [ "${1:-}" = "--health" ]; then
	exec bash -c 'exec 3<>"/dev/udp/127.0.0.1/$1" &&
		printf "SHTQ\002\001\000\000\000\001%0150d" 0 >&3 &&
		[ "$(timeout 3 head -c 4 <&3)" = SHTQ ]' health "$query_port"
fi

set -- --port "$port" --query-port "$query_port" \
	--map "${SHOOTER_MAP:-breakwater}" --mode "${SHOOTER_MODE:-tdm}" "$@"

[ -n "${SHOOTER_NAME:-}" ] && set -- --name "$SHOOTER_NAME" "$@"
[ -n "${SHOOTER_BOTS:-}" ] && set -- --bots "$SHOOTER_BOTS" "$@"
[ -n "${SHOOTER_BOT_SKILL:-}" ] && set -- --bot-skill "$SHOOTER_BOT_SKILL" "$@"
[ -n "${SHOOTER_SCORE_LIMIT:-}" ] && set -- --score-limit "$SHOOTER_SCORE_LIMIT" "$@"
[ -n "${SHOOTER_TIME_LIMIT:-}" ] && set -- --time-limit "$SHOOTER_TIME_LIMIT" "$@"
[ -n "${SHOOTER_ROUNDS:-}" ] && set -- --rounds "$SHOOTER_ROUNDS" "$@"
[ -n "${SHOOTER_CUSTOM:-}" ] && set -- --custom "$SHOOTER_CUSTOM" "$@"
[ -n "${SHOOTER_TEAM_BOTS:-}" ] && set -- --team-bots "$SHOOTER_TEAM_BOTS" "$@"
[ -n "${SHOOTER_TEAM_BOT_SKILL:-}" ] && set -- --team-bot-skill "$SHOOTER_TEAM_BOT_SKILL" "$@"
case "${SHOOTER_AUTO_START:-}" in
1 | true | yes) set -- --auto-start "$@" ;;
esac

# The dedicated server export always runs as a server, headless.
exec /opt/shooter/shooter-server.x86_64 --headless -- "$@"
