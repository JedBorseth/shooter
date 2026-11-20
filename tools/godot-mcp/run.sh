#!/bin/sh
# Starts the editor MCP bridge (stdio) for an agent; .mcp.json runs this.
# Installs its pinned dependencies on first use. Everything but the MCP
# protocol goes to stderr, so stdout stays clean.
set -eu
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if ! command -v node >/dev/null 2>&1; then
	echo "godot-mcp: Node.js 18 or newer is needed (https://nodejs.org)" >&2
	exit 1
fi
if [ ! -d "$dir/node_modules/@modelcontextprotocol/sdk" ] || [ ! -d "$dir/node_modules/zod" ]; then
	echo "godot-mcp: installing dependencies" >&2
	(cd "$dir" && npm ci --omit=dev --no-audit --no-fund >&2)
fi
exec node "$dir/src/index.js"
