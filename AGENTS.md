# Agent instructions

## Land every change

When a task changes files in this repository, finish it with the [ship-pr](.agents/skills/ship-pr/SKILL.md) skill: open a pull request, wait for every CI check to pass, fix any failures, and squash-merge it into `main`. Do this without being asked. Skip it only for read-only work, or when the user says not to push, not to merge, draft only, or local only.
