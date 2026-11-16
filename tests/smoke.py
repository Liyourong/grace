#!/usr/bin/env python3
"""Papers-focused HTTP smoke tests for Grace (Issue #17).

Usage:
    python3 tests/smoke.py [BASE_URL]

Defaults to GRACE_URL env var or http://localhost:8090.
"""

from __future__ import annotations

import http.cookiejar
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE_URL = (sys.argv[1] if len(sys.argv) > 1 else os.environ.get("GRACE_URL", "http://localhost:8090")).rstrip("/")


class Client:
    def __init__(self, base_url: str) -> None:
        self.base_url = base_url
        self.opener = urllib.request.build_opener(
            urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar())
        )

    def request(self, method: str, path: str, body: object | None = None) -> tuple[int, dict | None]:
        data = None
        headers: dict[str, str] = {}
        if method not in ("GET", "HEAD", "OPTIONS"):
            headers["Origin"] = self.base_url
        if body is not None:
            headers["Content-Type"] = "application/json"
            raw = body if isinstance(body, str) else json.dumps(body, ensure_ascii=False)
            data = raw.encode("utf-8")

        req = urllib.request.Request(f"{self.base_url}{path}", data=data, headers=headers, method=method)
        try:
            with self.opener.open(req, timeout=10) as resp:
                raw_resp = resp.read()
                return resp.status, _try_json(raw_resp)
        except urllib.error.HTTPError as err:
            raw_resp = err.read()
            return err.code, _try_json(raw_resp)


def _try_json(raw_resp: bytes) -> dict | None:
    if not raw_resp:
        return None
    try:
        return json.loads(raw_resp)
    except json.JSONDecodeError:
        return None


def wait_ready(client: Client) -> None:
    for _ in range(30):
        try:
            status, _ = client.request("GET", "/health/ready")
            if status == 200:
                return
        except urllib.error.URLError:
            pass
        time.sleep(1)
    raise RuntimeError(f"Service at {client.base_url} did not become ready in 30s")


def main() -> None:
    suffix = str(int(time.time() * 1000))
    user_a = Client(BASE_URL)
    user_b = Client(BASE_URL)

    wait_ready(user_a)

    # Sign in two distinct users.
    status, data = user_a.request("POST", "/api/auth/register", {"username": f"smoke_a_{suffix}", "password": "smoke-password-123"})
    assert status in (200, 201), f"user A register failed: {status} {data}"
    status, data = user_b.request("POST", "/api/auth/register", {"username": f"smoke_b_{suffix}", "password": "smoke-password-123"})
    assert status in (200, 201), f"user B register failed: {status} {data}"

    # Default paper is provisioned once on first list.
    status, list_a = user_a.request("GET", "/api/papers")
    assert status == 200 and len(list_a["papers"]) == 1, f"expected 1 default paper: {status} {list_a}"
    status, list_a_again = user_a.request("GET", "/api/papers")
    assert status == 200 and len(list_a_again["papers"]) == 1, f"default paper duplicated: {list_a_again}"

    # 1. Invalid upload returns 400 with at least three details.
    invalid_paper = json.loads((ROOT / "examples" / "invalid-paper.json").read_text(encoding="utf-8"))
    status, err = user_a.request("POST", "/api/papers", invalid_paper)
    assert status == 400, f"expected 400 for invalid-paper.json, got {status}: {err}"
    assert err["error"]["code"] == "invalid_paper_file", err
    assert err["error"]["message"] == "试卷文件格式不正确", err
    assert len(err["error"].get("details", [])) >= 3, f"expected >= 3 details, got {err}"

    # 2. NUL input returns 400.
    valid_paper = json.loads((ROOT / "examples" / "french-basics.json").read_text(encoding="utf-8"))
    nul_paper = json.loads(json.dumps(valid_paper, ensure_ascii=False))
    nul_paper["title"] = "法语\0基础"
    status, err = user_a.request("POST", "/api/papers", nul_paper)
    assert status == 400, f"expected 400 for NUL input, got {status}: {err}"
    assert err["error"]["code"] == "invalid_paper_file", err

    # 3. Uploading the same file twice produces distinct paper IDs.
    status1, res1 = user_a.request("POST", "/api/papers", valid_paper)
    status2, res2 = user_a.request("POST", "/api/papers", valid_paper)
    assert status1 == 201 and status2 == 201, (status1, res1, status2, res2)
    pid1 = res1["paper"]["id"]
    pid2 = res2["paper"]["id"]
    assert pid1 != pid2, f"expected distinct paper IDs, got {pid1} == {pid2}"

    # 4. A forged owner_id is ignored: paper belongs to signed-in owner (A), not B or forged id.
    forged_paper = dict(valid_paper, owner_id=999999)
    status, res_forged = user_a.request("POST", "/api/papers", forged_paper)
    assert status == 201, f"forged owner_id should be ignored, got {status}: {res_forged}"
    forged_id = res_forged["paper"]["id"]
    status, _ = user_a.request("GET", f"/api/papers/{forged_id}")
    assert status == 200, f"owner A could not open paper {forged_id}: {status}"
    status, _ = user_b.request("GET", f"/api/papers/{forged_id}")
    assert status == 404, f"user B should get 404 for user A's paper, got {status}"

    # 5. Question IDs/positions are preserved and unanswered questions reveal no answer key or feedback.
    status, detail = user_a.request("GET", f"/api/papers/{pid1}")
    assert status == 200, f"GET paper detail failed: {status} {detail}"
    assert detail["current_question_id"] == "q1", detail
    questions = detail["questions"]
    assert [q["id"] for q in questions] == ["q1", "q2", "q3"], questions
    assert [q["position"] for q in questions] == [1, 2, 3], questions
    for q in questions:
        assert q["answer"] is None, f"unanswered question should have null answer: {q}"
        for forbidden in ("correct_option_id", "accepted_answers", "correct_answer", "explanation"):
            assert forbidden not in q, f"unanswered question leaked {forbidden}: {q}"

    # Answer q2 using its preserved author ID and verify only q2 gets feedback (without internal keys).
    status, ans_res = user_a.request("PUT", f"/api/papers/{pid1}/answers/q2", {"text": "Merci"})
    assert status == 200, f"PUT answer by preserved ID q2 failed: {status} {ans_res}"
    status, detail_after = user_a.request("GET", f"/api/papers/{pid1}")
    assert status == 200, detail_after
    q_by_id = {q["id"]: q for q in detail_after["questions"]}
    assert q_by_id["q1"]["answer"] is None
    assert q_by_id["q3"]["answer"] is None
    assert q_by_id["q2"]["answer"]["saved_answer"] == "Merci"
    assert q_by_id["q2"]["answer"]["correct_answer"] == "Merci."
    assert q_by_id["q2"]["answer"]["explanation"]
    for q in detail_after["questions"]:
        assert "correct_option_id" not in q
        assert "accepted_answers" not in q

    print("All papers smoke checks passed!")


if __name__ == "__main__":
    main()
