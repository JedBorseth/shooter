#!/usr/bin/env python3
"""Private crash inbox on the public host, behind its HTTPS reverse proxy.

Bind only to a private interface; the proxy must allow only POST /shooter/crashes,
buffer requests with a 2 MiB + 512 byte limit, and enforce a request timeout.
No download/list endpoint, client-chosen file paths, credentials, or executable data.
"""

import argparse
from collections import deque
from http.server import BaseHTTPRequestHandler, HTTPServer
import os
from pathlib import Path
import tempfile
import time

MAX_BYTES = 2 * 1024 * 1024 + 512
KEEP_REPORTS = 200


class Inbox(HTTPServer):
    def __init__(self, address, directory):
        self.directory = Path(directory)
        self.directory.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.arrivals = deque()
        super().__init__(address, Handler)

    def get_request(self):
        sock, address = super().get_request()
        sock.settimeout(10)
        return sock, address


class Handler(BaseHTTPRequestHandler):
    server_version = "ShooterCrashInbox"

    def do_POST(self):
        if self.path != "/shooter/crashes":
            self.send_error(404)
            return
        if self.headers.get("Transfer-Encoding"):
            self.send_error(400)
            return
        try:
            length = int(self.headers.get("Content-Length", "-1"))
        except ValueError:
            length = -1
        if not 0 < length <= MAX_BYTES:
            self.send_error(413)
            return
        if self.headers.get_content_type() != "text/plain":
            self.send_error(415)
            return
        now = time.monotonic()
        while self.server.arrivals and now - self.server.arrivals[0] >= 60:
            self.server.arrivals.popleft()
        if len(self.server.arrivals) >= 30:
            self.send_error(429)
            return
        self.server.arrivals.append(now)
        try:
            body = self.rfile.read(length)
            if len(body) != length:
                self.send_error(400)
                return
            # Validate text, never deserialize objects or interpret report content.
            body.decode("utf-8")
            with tempfile.NamedTemporaryFile(
                prefix=f"crash-{time.time_ns()}-",
                suffix=".log",
                dir=self.server.directory,
                delete=False,
            ) as report:
                report.write(body)
                report.flush()
                os.fsync(report.fileno())
            logs = sorted(self.server.directory.glob("crash-*.log"))
            for old in logs[:-KEEP_REPORTS]:
                old.unlink(missing_ok=True)
        except (OSError, UnicodeError):
            self.send_error(400)
            return
        self.send_response(201)
        self.send_header("Content-Length", "0")
        self.send_header("Cache-Control", "no-store")
        self.end_headers()

    def log_message(self, format, *args):
        # Do not put report bodies, paths supplied by clients, or IPs in journals.
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=27011)
    parser.add_argument("--dir", type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    with Inbox((args.bind, args.port), args.dir) as server:
        server.serve_forever()


if __name__ == "__main__":
    main()
