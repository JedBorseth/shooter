---
name: ship-pr
description: Publish and land finished work in this repo. Use at the end of every task that changes files here, without being asked; skip it only for read-only work or when the user says not to push, not to merge, draft only, or local only. Commits to a branch, opens a pull request, waits for every CI check to pass, fixes failures, rebases onto main, and squash-merges.
---

# Ship a pull request

Every piece of work that changes files in this repository ends merged into `main`. The user does not review or merge by hand. Run this skill when the work is done, without waiting for another prompt.

Skip the skill, or stop at the step the user names, when:

- the task was read-only (questions, investigations, reviews);
- the user said not to commit, not to push, not to merge, draft only, local only, or "hold".

In those cases, say in the final reply what you left unpublished.

## Repository facts

- Default branch: `main`. There is no branch protection, so GitHub will not wait for CI on its own. You wait for the checks, then merge.
- Merge method: squash. The repository deletes the head branch after a merge, so do not pass `--delete-branch` (it fails in a worktree because `main` is checked out elsewhere).
- CI comes from two workflows, and every check from both must pass:
  - `CI` (`.github/workflows/ci.yml`): Rust fmt and clippy, Rust tests on Linux, Windows, and macOS, GDScript lint and format, and a headless Godot smoke test.
  - `Build` (`.github/workflows/build.yml`): GDExtension builds and client exports for Linux, Windows, and macOS. This is the slowest workflow.
- The repository is public. Never write "slambox" in any form in commits, PR titles, or PR bodies. Call the machine "Josh's dev box" or leave it out.
- Other agents work in parallel and land their own PRs. Merge one PR at a time, and always rebase onto the latest `main` before you merge.

## Steps

### 1. Commit on a branch

If you are on `main`, create a branch first (`git switch -c <short-topic>`). Agents in T3 Code already have a `t3/...` branch; use it.

Commit with a message that says what changed and why, in the imperative ("Add ammo pickups to the arena"). End the message with the attribution trailer your harness gives you, if any.

### 2. Run the local checks for what you touched

Fix failures before you push. Run only what applies:

- Rust changed: from `rust/`, run `cargo fmt --all`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --locked`.
- GDScript changed: `pipx run --spec gdtoolkit==4.5.0 gdlint godot/scripts godot/tests` and `pipx run --spec gdtoolkit==4.5.0 gdformat --check godot/scripts godot/tests`.
- Scenes, assets, or the extension changed: if Godot is installed, build the extension (`cargo build -p shooter-game`) and run `godot --headless --path godot --script res://tests/smoke_test.gd`. Otherwise leave it to CI.

Leave the client export builds to CI.

### 3. Rebase and push

```sh
git fetch origin
git rebase origin/main
git push --force-with-lease -u origin HEAD
```

Resolve conflicts by keeping both sides' intent. Read the other PR's change if you need to. Never drop someone else's work to make a rebase pass. Rerun the step 2 checks if the rebase changed files you touched.

### 4. Open the pull request

If a PR for the branch already exists (`gh pr view`), reuse it. Otherwise:

```sh
gh pr create --base main --title "<what changed, plain words>" --body-file <file>
```

The body has a short summary of what changed and why, a "Testing" section listing what you ran, and new dependencies or assets with their licenses. End it with the PR attribution line your harness gives you, if any. Do not open it as a draft.

In T3 Code, link the PR to the thread with `link_pull_request` right after you create it.

### 5. Wait for every check

Get the head commit: `git rev-parse HEAD`.

- In T3 Code: call `watch_pull_request` on the PR and end your turn. You are woken when checks finish. Do not poll, sleep, or run `gh pr checks --watch` there. A wake that says "required checks passed" can arrive while slower checks (the macOS build, the client export) are still running. On every wake, run `gh pr checks <number>`. If anything is still pending, end your turn again; the watch stays on.
- Elsewhere: `gh pr checks <number> --watch --fail-fast --interval 30`, with a timeout of at least 30 minutes.

The checks are done when every check on the current head commit has finished. If none are listed yet, the workflows have not registered; wait, do not treat that as green. `success`, `skipped`, and `neutral` count as passing.

### 6. Fix failures

For each failed check:

1. Read the failure: `gh run view <run-id> --log-failed` (find run IDs with `gh pr checks <number>`).
2. Fix the cause in code, commit, push, and go back to step 5.

If a failure is clearly unrelated to your change (a runner outage, a network timeout during a download), rerun it once with `gh run rerun <run-id> --failed`. If it fails again, treat it as real.

Never disable, skip, or weaken a check, lint rule, or test to get green. If a check is broken on `main` itself, fix it in this PR and say so in the body.

### 7. Make sure the PR is current, then merge

Before you merge, check that `main` has not moved since your last push:

```sh
git fetch origin
git merge-base --is-ancestor origin/main HEAD && echo current || echo behind
```

If it is behind, go back to step 3. CI must pass on the rebased commit, not just on an older one.

Do not merge if the PR is a draft, has a "changes requested" review, or the user asked you to hold it.

When it is current and every check passed:

```sh
gh pr merge <number> --squash --match-head-commit "$(git rev-parse HEAD)"
```

`--match-head-commit` stops the merge if someone pushed to the branch after your checks ran.

### 8. Confirm and report

Run `gh pr view <number> --json state,mergeCommit` and confirm the state is `MERGED`. In T3 Code, call `unwatch_pull_request` on the PR.

In the final reply, give the PR URL, the merge commit, and one line on what CI ran. If you stopped early because of a limit in "Skip the skill", say where you stopped and what is left.
