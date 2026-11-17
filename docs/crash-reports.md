# Client crash reports

Start the game through `shooter-launcher` (including the desktop shortcut).
The launcher saves each session's stdout and stderr in
`<Shooter data directory>/<platform>/logs/client-*.log`. On Windows this is
`%LOCALAPPDATA%/Shooter/windows-x86_64/logs`. Twenty sessions are retained,
outside downloaded build folders. Godot's own twenty rotating logs remain in
`user://logs`; local match servers use `local-server.log` so they do not rotate
the client's log out from under it.

Logs include the build ID, UTC startup time, engine version, OS, GPU/API,
verbose engine output, join milestones, five-second scene/FPS/memory and
network stats, errors, panic backtraces, and the process exit status. We do not
add logging of chat, player names, input contents, environment variables, or
command-line arguments. Existing engine diagnostics may contain local paths;
the uploader replaces the user's home directory with `<home>`.

After a nonzero process exit or fatal signal, the launcher sends the last 2 MiB
plus the build header over verified HTTPS to
`https://borseth.ddns.net/shooter/crashes`. A successful normal close does not
create or send a report. Fixing the GDExtension's cached engine resources being
dropped after shutdown prevents normal closes from aborting and being mistaken
for crashes. Forced process termination may be indistinguishable from a crash;
killing the launcher itself or losing power cannot upload immediately.

Failed uploads are saved as `.pending` files next to logs. The next launch
retries at most two, even if that next session exits normally. Pending reports
expire with their corresponding session after twenty newer sessions. Uploads
verify certificate trust and hostname, refuse HTTP and redirects, and have a
15-second total deadline. Override with `--crash-url https://...` or
`SHOOTER_CRASH_URL`; an empty value disables uploads. Direct Godot/executable
launches retain engine logs but need the launcher to supervise native crashes.

## Master host inbox

`tools/crash_receiver.py` runs alongside the UDP directory on the master host.
It accepts only plain UTF-8 POST bodies at `/shooter/crashes`, with a
2 MiB + 512 byte maximum, ten-second socket timeout, thirty reports/minute
global rate limit, and a maximum of 200 private report files (about 400 MiB).
It has no read/list endpoint and never uses client input as a filename.
No shared credential is embedded in the public game binary; this is a public,
write-only inbox. Reports are untrusted diagnostic text, never executed.

Install the script in `~/.local/share/shooter-crashes/` and the supplied
`tools/shooter-crashes.service` in `~/.config/systemd/user/`. Enable it with
`systemctl --user enable --now shooter-crashes`. Reports are in
`~/.local/share/shooter-crashes/reports`, directory mode 0700, files mode 0600.
Read them through SSH, not a public web directory. The default listener is
loopback port 27011; when the TLS proxy is containerized, bind only its private
bridge gateway using a service override. Never expose this HTTP port publicly.

Inside the existing HTTPS site's Caddy block, before its catch-all route:

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
Validate and reload Caddy; keep its existing trusted TLS certificate and HTTPS
redirect. The UDP directory protocol and ports do not carry reports.

Receiver checks: `python -m unittest discover -s tools -p test_crash_receiver.py -v`.
Launcher checks: `cargo test --locked -p shooter-launcher` from `rust/`.
