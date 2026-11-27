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

- Default branch: `main`. A ruleset ("Protect main") requires a pull request and passing checks. GitHub auto-merge waits for them. You still confirm every check yourself, including the ones the ruleset does not list.
- Merge method: squash. The repository deletes the head branch after a merge, so do not pass `--delete-branch` (it fails in a worktree because `main` is checked out elsewhere).
- CI is one workflow, `CI` (`.github/workflows/ci.yml`): lint and tests on Linux, Rust tests on Windows and macOS, every headless Godot test, GDExtension builds, and client exports. Its last job, `CI passed`, passes only when every other job did; that is the check the ruleset requires. [docs/ci.md](../../../docs/ci.md) describes the pipeline.
- Godot tests are discovered: every `godot/tests/*_test.gd` runs in CI. Add a test by adding the file; do not edit `ci.yml` for it. Tests step the game at a fixed frame rate; a test that starts another process (a dedicated server) must be listed in `REALTIME` in `.github/scripts/godot-tests.sh`.
- The repository is public. Never write "slambox" in any form in commits, PR titles, or PR bodies. Call the machine "Josh's dev box" or leave it out.
- Other agents work in parallel and land their own PRs. If `main` has a merge queue, it tests each PR on top of the latest `main` for you. Without one, merge one PR at a time and rebase onto the latest `main` before you merge.

## Steps

### 1. Commit on a branch

If you are on `main`, create a branch first (`git switch -c <short-topic>`). Agents in T3 Code already have a `t3/...` branch; use it.

Commit with a message that says what changed and why, in the imperative ("Add ammo pickups to the arena"). End the message with the attribution trailer your harness gives you, if any.

### 2. Run the local checks for what you touched

Fix failures before you push. Run only what applies:

- Rust changed: from `rust/`, run `cargo fmt --all`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --locked`.
- GDScript changed: `pipx run --spec gdtoolkit==4.5.0 gdlint godot/scripts godot/tests` and `pipx run --spec gdtoolkit==4.5.0 gdformat --check godot/scripts godot/tests`.
- Scenes, assets, or the extension changed: if Godot is installed, build the extension (`cargo build -p shooter-game`) and run the Godot tests you touched the way CI does: `just test smoke_test <other_test> ...` (or `.github/scripts/godot-tests.sh smoke_test ...` from the repository root). Otherwise leave it to CI.

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

The body has a short summary of what changed and why, a "Call of Duty reference" section for any feature or visual change (see the [cod-design](../cod-design/SKILL.md) skill), a "Testing" section listing what you ran, and new dependencies or assets with their licenses. End it with the PR attribution line your harness gives you, if any. Do not open it as a draft.

In T3 Code, link the PR to the thread with `link_pull_request` right after you create it.

Then turn on auto-merge, so GitHub merges (or, with a merge queue, queues the PR) as soon as the required checks pass:

```sh
gh pr merge <number> --auto --squash
```

Skip auto-merge if the user asked you to hold the PR.

### 5. Wait for every check

Get the head commit: `git rev-parse HEAD`.

- In T3 Code: call `watch_pull_request` on the PR and end your turn. You are woken when checks finish. Do not poll, sleep, or run `gh pr checks --watch` there. A wake that says "required checks passed" can arrive before every job has finished. On every wake, run `gh pr checks <number>`. If anything is still pending, end your turn again; the watch stays on.
- Elsewhere: `gh pr checks <number> --watch --fail-fast --interval 30`, with a timeout of at least 30 minutes.

The checks are done when every check on the current head commit has finished. If none are listed yet, the workflows have not registered; wait, do not treat that as green. `success`, `skipped`, and `neutral` count as passing.

### 6. Fix failures

For each failed check:

1. Read the failure: `gh run view <run-id> --log-failed` (find run IDs with `gh pr checks <number>`).
2. Fix the cause in code, commit, push, and go back to step 5.

If a failure is clearly unrelated to your change (a runner outage, a network timeout during a download), rerun it once with `gh run rerun <run-id> --failed`. If it fails again, treat it as real.

Never disable, skip, or weaken a check, lint rule, or test to get green. If a check is broken on `main` itself, fix it in this PR and say so in the body.

### 7. Make sure the PR is current, then merge

First check whether `main` has a merge queue:

```sh
gh api graphql -f query='{repository(owner:"JedBorseth",name:"shooter"){mergeQueue(branch:"main"){id}}}' -q '.data.repository.mergeQueue.id'
```

**With a merge queue** (the command prints an ID): do not rebase just because `main` moved. Auto-merge puts the PR in the queue once its checks pass, and the queue runs CI on the PR merged with the latest `main` before landing it. Rebase only when the PR has conflicts (`gh pr view <number> --json mergeable` shows `CONFLICTING`) or the queue removes it for a failure; then fix, push, and go back to step 5. Wait for the PR to merge the same way you waited for checks.

**Without a merge queue** (the command prints nothing): before you merge, check that `main` has not moved since your last push:

```sh
git fetch origin
git merge-base --is-ancestor origin/main HEAD && echo current || echo behind
```

If it is behind, go back to step 3. CI must pass on the rebased commit. If someone else pushed to your branch (for example, GitHub's "Update branch" merge), run `git pull --ff-only` first and look at what they changed.

Do not merge if the PR is a draft, has a "changes requested" review, or the user asked you to hold it.

Auto-merge usually lands the PR by itself. If `gh pr view <number> --json state` still shows `OPEN` once it is current and every check has passed, merge it yourself:

```sh
gh pr merge <number> --squash --match-head-commit "$(git rev-parse HEAD)"
```

`--match-head-commit` stops the merge if someone pushed to the branch after your checks ran. If the merge is refused, read the reason (`gh pr view <number> --json mergeStateStatus`) and fix that. Never use `--admin` and never change the ruleset to get a merge through.

### 8. Confirm and report

Run `gh pr view <number> --json state,mergeCommit` and confirm the state is `MERGED`. In T3 Code, call `unwatch_pull_request` on the PR.

In the final reply, give the PR URL, the merge commit, and one line on what CI ran. If you stopped early because of a limit in "Skip the skill", say where you stopped and what is left.
