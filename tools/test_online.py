"""Checks the Nakama modules in docker/online against a running stack.

Start the stack first (docker compose -f docker/online/compose.yml up -d
--wait), then run `python tools/test_online.py`. NAKAMA_URL, NAKAMA_SERVER_KEY,
and NAKAMA_HTTP_KEY default to the stack's development values. Standard library
only. CI runs it in the "Online services" job.
"""

import base64
import json
import os
import unittest
import urllib.error
import urllib.parse
import urllib.request
import uuid

URL = os.environ.get("NAKAMA_URL", "http://127.0.0.1:7350")
SERVER_KEY = os.environ.get("NAKAMA_SERVER_KEY", "defaultkey")
HTTP_KEY = os.environ.get("NAKAMA_HTTP_KEY", "localdevhttpkey")


class NakamaError(Exception):
    def __init__(self, status: int, body: str):
        super().__init__(f"{status}: {body}")
        self.status = status
        self.body = body


def request(method: str, path: str, body=None, token=None, basic=None, raw=None):
    data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
    req = urllib.request.Request(URL + path, data=data, method=method)
    req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", "Bearer " + token)
    elif basic:
        req.add_header("Authorization", "Basic " + base64.b64encode(basic.encode()).decode())
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            text = response.read().decode()
    except urllib.error.HTTPError as error:
        raise NakamaError(error.code, error.read().decode()) from None
    return json.loads(text) if text else {}


def device_session(username=None) -> dict:
    query = {"create": "true"}
    if username:
        query["username"] = username
    return request(
        "POST",
        "/v2/account/authenticate/device?" + urllib.parse.urlencode(query),
        {"id": str(uuid.uuid4())},
        basic=SERVER_KEY + ":",
    )


def rpc(token: str, name: str, payload: dict) -> dict:
    # A session's RPC body is the payload as a JSON string.
    result = request("POST", f"/v2/rpc/{name}", json.dumps(payload), token=token)
    return json.loads(result.get("payload") or "{}")


def server_rpc(name: str, payload: dict, key: str = HTTP_KEY) -> dict:
    query = urllib.parse.urlencode({"http_key": key, "unwrap": ""})
    return request("POST", f"/v2/rpc/{name}?{query}", raw=json.dumps(payload).encode())


def user_id(token: str) -> str:
    return request("GET", "/v2/account", token=token)["user"]["id"]


class ProgressTest(unittest.TestCase):
    def setUp(self):
        self.session = device_session("t" + uuid.uuid4().hex[:12])
        self.token = self.session["token"]

    def test_new_account_starts_empty(self):
        self.assertEqual(rpc(self.token, "progress_get", {}), {"xp": 0, "weapon_kills": {}})

    def test_import_once_into_an_empty_account(self):
        got = rpc(self.token, "progress_import", {"xp": 5000, "weapon_kills": {"m4": 12, "Bad Key": 3}})
        self.assertEqual(got, {"xp": 5000, "weapon_kills": {"m4": 12}})
        again = rpc(self.token, "progress_import", {"xp": 90000, "weapon_kills": {"m4": 99}})
        self.assertEqual(again["xp"], 5000, "a second import changes nothing")

    def test_import_is_capped_at_the_top_level(self):
        got = rpc(self.token, "progress_import", {"xp": 10**9})
        self.assertEqual(got["xp"], 604800)

    def test_players_cannot_write_progress(self):
        with self.assertRaises(NakamaError):
            request(
                "PUT",
                "/v2/storage",
                {"objects": [{"collection": "progress", "key": "progress", "value": '{"xp": 999999}'}]},
                token=self.token,
            )
        self.assertEqual(rpc(self.token, "progress_get", {})["xp"], 0)

    def test_a_reported_match_is_capped_by_time(self):
        # The account is seconds old: almost nothing may be awarded yet.
        got = rpc(self.token, "progress_match", {"xp": 50000, "weapon_kills": {"m4": 40}})
        self.assertLess(got["xp"], 1500)
        self.assertLessEqual(sum(got["weapon_kills"].values()), 20)

    def test_server_rpcs_need_the_http_key(self):
        uid = user_id(self.token)
        with self.assertRaises(NakamaError):
            server_rpc("server_progress_add", {"user_id": uid, "xp": 100}, key="wrong")
        with self.assertRaises(NakamaError):
            rpc(self.token, "server_progress_add", {"user_id": uid, "xp": 100})

    def test_server_awards_and_reads_progress(self):
        uid = user_id(self.token)
        got = server_rpc("server_progress_add", {"user_id": uid, "xp": 2400, "weapon_kills": {"ak47": 7}})
        self.assertEqual(got, {"xp": 2400, "weapon_kills": {"ak47": 7}})
        server_rpc("server_progress_add", {"user_id": uid, "xp": 600, "weapon_kills": {"ak47": 1, "m870": 2}})
        self.assertEqual(
            server_rpc("server_progress_get", {"user_id": uid}),
            {"xp": 3000, "weapon_kills": {"ak47": 8, "m870": 2}},
        )
        self.assertEqual(rpc(self.token, "progress_get", {})["xp"], 3000)
        # Progress the server awarded closes the one-time import.
        self.assertEqual(rpc(self.token, "progress_import", {"xp": 1})["xp"], 3000)

    def test_server_rpcs_reject_unknown_players(self):
        with self.assertRaises(NakamaError):
            server_rpc("server_progress_get", {"user_id": str(uuid.uuid4())})

    def test_classes_are_saved_on_the_account(self):
        classes = {"custom": [{"name": "Rush", "primary": "pdw9"}], "selected": "custom:0"}
        request(
            "PUT",
            "/v2/storage",
            {
                "objects": [
                    {
                        "collection": "loadout",
                        "key": "classes",
                        "value": json.dumps(classes),
                        "permission_read": 1,
                        "permission_write": 1,
                    }
                ]
            },
            token=self.token,
        )
        uid = user_id(self.token)
        got = request(
            "POST",
            "/v2/storage",
            {"object_ids": [{"collection": "loadout", "key": "classes", "user_id": uid}]},
            token=self.token,
        )
        self.assertEqual(json.loads(got["objects"][0]["value"]), classes)


if __name__ == "__main__":
    unittest.main(verbosity=2)
