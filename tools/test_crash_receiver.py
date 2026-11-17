import http.client
from pathlib import Path
import tempfile
import threading
import unittest

from crash_receiver import Inbox, MAX_BYTES


class CrashInboxTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.server = Inbox(("127.0.0.1", 0), self.directory.name)
        self.thread = threading.Thread(target=self.server.serve_forever)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
        self.directory.cleanup()

    def request(self, method, path, body=None, headers=None):
        client = http.client.HTTPConnection(*self.server.server_address, timeout=2)
        client.request(method, path, body, headers or {})
        response = client.getresponse()
        status = response.status
        response.read()
        client.close()
        return status

    def test_stores_report_privately_and_never_serves_it(self):
        body = "Shooter build test\ncrash details"
        self.assertEqual(self.request("POST", "/shooter/crashes", body, {"Content-Type": "text/plain"}), 201)
        reports = list(Path(self.directory.name).glob("*.log"))
        self.assertEqual(len(reports), 1)
        self.assertEqual(reports[0].read_text(), body)
        self.assertEqual(self.request("GET", "/shooter/crashes"), 501)
        self.assertEqual(self.request("POST", "/../escape", body), 404)

    def test_rejects_oversize_and_invalid_content(self):
        self.assertEqual(self.request("POST", "/shooter/crashes", b"", {"Content-Length": str(MAX_BYTES + 1)}), 413)
        self.assertEqual(self.request("POST", "/shooter/crashes", "json", {"Content-Type": "application/json"}), 415)
        self.assertEqual(self.request("POST", "/shooter/crashes", b"\xff", {"Content-Type": "text/plain"}), 400)
        self.assertEqual(list(Path(self.directory.name).iterdir()), [])

    def test_global_rate_limit(self):
        for _ in range(30):
            self.assertEqual(self.request("POST", "/shooter/crashes", "crash", {"Content-Type": "text/plain"}), 201)
        self.assertEqual(self.request("POST", "/shooter/crashes", "crash", {"Content-Type": "text/plain"}), 429)

    def test_retention_bounds_disk_usage(self):
        root = Path(self.directory.name)
        for i in range(205):
            (root / f"crash-{i:020}.log").write_text("old")
        self.assertEqual(self.request("POST", "/shooter/crashes", "new", {"Content-Type": "text/plain"}), 201)
        self.assertEqual(len(list(root.glob("*.log"))), 200)
        self.assertFalse((root / "crash-00000000000000000000.log").exists())


if __name__ == "__main__":
    unittest.main()
