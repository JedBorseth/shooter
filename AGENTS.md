# Agent instructions

## Design every feature like Call of Duty

Before you build or change any feature a player can see, hear, or feel, or the system behind it, follow the [cod-design](.agents/skills/cod-design/SKILL.md) skill: find how Call of Duty does it, match its rules, numbers, inputs, and edge cases, and record the reference in the docs and the PR. Visual work follows its graphics target: Treyarch's Black Ops 2 and 3 look with the best of the modern Modern Warfare games. Do this without being asked. A request from the user or a recorded project decision overrides it.

## Land every change

When a task changes files in this repository, finish it with the [ship-pr](.agents/skills/ship-pr/SKILL.md) skill: open a pull request, wait for every CI check to pass, fix any failures, and squash-merge it into `main`. Do this without being asked. Skip it only for read-only work, or when the user says not to push, not to merge, draft only, or local only.
