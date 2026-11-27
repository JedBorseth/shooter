# Agent instructions

## Stay inside the scope

[SCOPE.md](SCOPE.md) lists what is in the game and what was cut on purpose. Work only on what it lists under **In**. Do not add a mode, map, scorestreak, progression system, online service, or other feature outside it, or bring back a cut one, unless the user asks for it in the current task. If you think something is missing, ask; do not build it. When the user changes the scope, update SCOPE.md in the same pull request.

## Make in-scope features feel like Call of Duty

When you build or change an in-scope feature a player can see, hear, or feel, follow the [cod-design](.agents/skills/cod-design/SKILL.md) skill: find how Call of Duty does it, match its rules, numbers, inputs, and edge cases, and record the reference in the docs and the PR. Visual work follows its graphics target: Treyarch's Black Ops 2 and 3 look with the best of the modern Modern Warfare games. Call of Duty is the reference for how a feature feels, not a list of features to add, and the game uses its own maps, names, and art, never Call of Duty's. A request from the user or a recorded project decision overrides it.

## Land every change

When a task changes files in this repository, finish it with the [ship-pr](.agents/skills/ship-pr/SKILL.md) skill: open a pull request, wait for every CI check to pass, fix any failures, and squash-merge it into `main`. Do this without being asked. Skip it only for read-only work, or when the user says not to push, not to merge, draft only, or local only.

## Working on maps

To see or edit a map, use the editor through its MCP bridge ([docs/editor-mcp.md](docs/editor-mcp.md)): `just agent-editor` starts an editor, `.mcp.json` connects you, and `godot_render_view` shows the open scene from any eye position. Pick props from `godot/assets/props/catalog.json` (`python3 tools/prop_catalog.py --find <tags>`), and follow the [map-design](.agents/skills/map-design/SKILL.md) skill for layouts.
