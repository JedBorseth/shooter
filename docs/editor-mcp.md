# Driving the editor from an agent

Any contributor's coding agent can open scenes, inspect and change nodes, run
GDScript in the editor, and look at a map from any eye position, through the
Godot AI MCP bridge. Two parts:

- **The plugin**, `godot/addons/godot_ai_mcp/` (enabled in the project): a TCP
  server inside the editor on `127.0.0.1` only, port 6505 by default.
- **The bridge**, `tools/godot-mcp/`: a small Node.js MCP server (stdio) that an
  agent starts and that talks to the plugin. Vendored from
  [godot-ai-mcp](https://github.com/event-horizon-studio/godot-ai-mcp) 1.1.0
  (MIT) with this project's changes: a configurable port, a longer request
  timeout, and `godot_render_view`. Its dependencies are pinned in
  `package-lock.json`; `tools/godot-mcp/run.sh` installs them on first use
  (Node.js 18 or newer).

## Setting it up

1. Start an editor: `just agent-editor` (or `tools/agent_editor.sh`). It builds
   the Rust extension, starts the editor in the background, and waits until the
   plugin listens. It uses `$DISPLAY` if set, else a desktop you already have
   running (a remote desktop on a GPU server), else a virtual display with
   software OpenGL (slow, but it works without a GPU). `just agent-editor
   status` and `just agent-editor stop` check on it and stop it. Opening the
   editor yourself (`just edit`) works too: the plugin starts with it.
2. Connect your agent. Claude Code reads `.mcp.json` at the repository root and
   asks once to allow the `godot` server. For another agent, register the same
   command as a stdio MCP server, run from the repository root:
   `sh tools/godot-mcp/run.sh`. For Codex, for example, in
   `~/.codex/config.toml`:

   ```toml
   [mcp_servers.godot]
   command = "sh"
   args = ["/path/to/shooter/tools/godot-mcp/run.sh"]
   ```

Several editors at once (one per checkout or worktree): give each its own port
with `GODOT_MCP_PORT`, both when starting the editor and in the agent's
environment (the plugin and the bridge read the same variable):
`GODOT_MCP_PORT=6511 just agent-editor`. `GODOT_MCP_TIMEOUT_MS` (default
180000) is how long the bridge waits for the editor; opening a large map
blocks the editor for about 20 s.

## Looking at a map

`godot_render_view` renders the open scene into an offscreen viewport from any
camera, without the editor's grid and gizmos, and returns the picture: a feet
position in metres, a compass heading (0 = north = −Z, 90 = east = +X), a
pitch in degrees, and `eye_height` (1.6 standing by default, 1.0 crouched,
0 for a free camera). `orthographic_size` with pitch −90 gives a plan.
`godot_get_viewport_screenshot` still returns the editor's own viewport, gizmos
and all.

The lighting is the open scene's own (its WorldEnvironment and lights, or the
editor's preview sun), which is not always what players see: some maps set
their look at runtime (`godot/scripts/looks.gd`). For the exact in-game look,
render with the game's capture scripts instead (`godot/tests/capture_*.gd`).

## Care

- `godot_execute_gdscript` runs any code in the editor, and edits made through
  the bridge are real edits to the open scene. Save only what you mean to
  commit, and check `git status` afterwards.
- The plugin only listens on `127.0.0.1`. Never forward its port off the
  machine.
- Opening the editor may rewrite `godot/project.godot` in its own format; do not
  commit that unless it is the point of the change.
