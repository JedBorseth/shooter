# Crash reports

When the game crashes, the launcher sends the end of that session's log to a
private inbox on the public host. Nothing is sent for a normal close.

## Client logs

Start the game through `shooter-launcher` (or the desktop shortcut). The
launcher starts the game with `--verbose` and `RUST_BACKTRACE=1`, and saves
each session's stdout and stderr in
`<Shooter data directory>/<platform>/logs/client-<time>-<pid>.log`: on
Windows, `%LOCALAPPDATA%\Shooter\windows-x86_64\logs`; on macOS,
`~/Library/Application Support/Shooter/macos-universal/logs`; on Linux,
`~/.local/share/shooter/linux-x86_64/logs`. It keeps the last twenty
sessions, outside the build folders, so an update never deletes them
(`rust/launcher/src/logs.rs`).

Godot's own twenty rotating logs stay in `user://logs`. Local match servers
write `user://logs/local-server.log`, so they do not rotate the client's log
away.

A log holds the build id, verbose engine output, join milestones, errors,
panic backtraces, and the process's exit status, plus the `Diagnostics`
autoload's breadcrumbs (`godot/scripts/diagnostics.gd`): the UTC start time,
engine version, OS, renderer, GPU and graphics API, and every five seconds
the scene, FPS, memory, and, in a match, the network stats. The game adds
no logging of chat, player names, input contents, environment variables, or
command-line arguments. Engine diagnostics may contain local paths; the
uploader replaces the user's home directory with `<home>`.

Starting the game executable or Godot directly keeps the engine logs, but
only the launcher sees a native crash and reports it.

## Uploads

After a nonzero exit or a fatal signal, the launcher sends the log's first
line (the build) and its last 2 MiB to
`https://borseth.ddns.net/shooter/crashes` (`rust/launcher/src/crash.rs`).
Uploads accept only HTTPS with a verified certificate and hostname, follow no
redirects, and give up after 15 seconds. Killing the game from outside may
look like a crash; killing the launcher itself, or losing power, sends
nothing.

A failed upload stays beside the log as a `.pending` file. The next launch
retries at most two, whether or not that session crashes, and a pending
report goes with its log once twenty newer sessions push it out.

`--crash-url https://…` or `SHOOTER_CRASH_URL` sends reports elsewhere; an
empty value turns uploads off.

## The inbox on the public host

`tools/crash_receiver.py` runs on `borseth.ddns.net`, beside its dedicated
server. It accepts only
plain UTF-8 `POST` bodies at `/shooter/crashes`, up to 2 MiB plus 512 bytes,
with a ten-second socket timeout, a limit of thirty reports a minute, and at
most 200 report files kept (about 400 MiB). It has no read or list endpoint,
never uses client input as a file name, and logs nothing about requests. No
credential is built into the public game: the inbox is public and
write-only, and reports are untrusted text, never executed.

Install the script in `~/.local/share/shooter-crashes/` and
`tools/shooter-crashes.service` in `~/.config/systemd/user/`, then enable it
with `systemctl --user enable --now shooter-crashes`. Reports land in
`~/.local/share/shooter-crashes/reports` (directory mode 0700, files 0600).
Read them over SSH, never from a public web directory. The receiver listens
on `127.0.0.1:27011` (`--bind`, `--port`); when the TLS proxy runs in a
container, bind only its private bridge gateway with a service override.
Never expose this HTTP port publicly.

Inside the HTTPS site's Caddy block, before its catch-all route:

```caddyfile
handle /shooter/crashes {
    request_body {
        max_size 2097664
    }
    reverse_proxy 127.0.0.1:27011 {
        request_buffers 2097664
        transport http {
            dial_timeout 3s
            response_header_timeout 15s
            read_timeout 15s
            write_timeout 15s
        }
    }
}
```

Use the private bridge gateway instead of loopback when Caddy is in Docker.
Validate and reload Caddy, keeping its trusted TLS certificate and HTTPS
redirect. The game server's UDP ports carry no reports.

## Tests

```sh
python -m unittest discover -s tools -p test_crash_receiver.py -v   # the inbox
(cd rust && cargo test --locked -p shooter-launcher)                # logs and uploads
```

CI runs both.
