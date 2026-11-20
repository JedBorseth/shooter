#!/bin/sh
# Starts the dedicated server with options from SHOOTER_* environment
# variables, then any arguments given to `docker run` (they win, because the
# server reads the last value of a repeated option). `--health` instead asks
# the running server's query port whether it answers (the image's health check).
set -eu

port=${SHOOTER_PORT:-27015}
query_port=${SHOOTER_QUERY_PORT:-$((port + 1))}

if [ "${1:-}" = "--health" ]; then
	exec /opt/shooter/shooter-query "127.0.0.1:${query_port}" --timeout 3
fi

set -- --port "$port" --query-port "$query_port" \
	--map "${SHOOTER_MAP:-slamcity}" --mode "${SHOOTER_MODE:-tdm}" "$@"

[ -n "${SHOOTER_NAME:-}" ] && set -- --name "$SHOOTER_NAME" "$@"
[ -n "${SHOOTER_BOTS:-}" ] && set -- --bots "$SHOOTER_BOTS" "$@"
[ -n "${SHOOTER_BOT_SKILL:-}" ] && set -- --bot-skill "$SHOOTER_BOT_SKILL" "$@"
[ -n "${SHOOTER_MASTER:-}" ] && set -- --master "$SHOOTER_MASTER" "$@"
[ -n "${SHOOTER_SCORE_LIMIT:-}" ] && set -- --score-limit "$SHOOTER_SCORE_LIMIT" "$@"
[ -n "${SHOOTER_TIME_LIMIT:-}" ] && set -- --time-limit "$SHOOTER_TIME_LIMIT" "$@"
[ -n "${SHOOTER_ROUNDS:-}" ] && set -- --rounds "$SHOOTER_ROUNDS" "$@"
[ -n "${SHOOTER_CUSTOM:-}" ] && set -- --custom "$SHOOTER_CUSTOM" "$@"
[ -n "${SHOOTER_TEAM_BOTS:-}" ] && set -- --team-bots "$SHOOTER_TEAM_BOTS" "$@"
[ -n "${SHOOTER_TEAM_BOT_SKILL:-}" ] && set -- --team-bot-skill "$SHOOTER_TEAM_BOT_SKILL" "$@"
# Accounts: the HTTP key stays in SHOOTER_ACCOUNTS_KEY (read by the server).
[ -n "${SHOOTER_ACCOUNTS:-}" ] && set -- --accounts "$SHOOTER_ACCOUNTS" "$@"
[ -n "${SHOOTER_EXIT_WHEN_EMPTY:-}" ] && set -- --exit-when-empty "$SHOOTER_EXIT_WHEN_EMPTY" "$@"
case "${SHOOTER_AUTO_START:-}" in
1 | true | yes) set -- --auto-start "$@" ;;
esac

# The dedicated server export always runs as a server, headless.
exec /opt/shooter/shooter-server.x86_64 --headless -- "$@"
