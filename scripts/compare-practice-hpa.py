"""Compare fixed replicas with the normal HPA in the grace-dash-demo minikube profile."""
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from http.client import HTTPConnection, HTTPException
from pathlib import Path
import argparse
import json
import math
import os
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
KUBE = ["kubectl", "--context", "grace-dash-demo", "-n", "grace", "--request-timeout=8s"]
SECONDS = 150
BIN = 10


def kube(*args):
    return subprocess.check_output([*KUBE, *args], text=True, timeout=12)


def limit(old, new):
    ops = [{"op": "test", "path": "/spec/maxReplicas", "value": old},
           {"op": "replace", "path": "/spec/maxReplicas", "value": new}]
    kube("patch", "hpa", "app", "--type=json", "-p", json.dumps(ops))


class BadResponse(Exception):
    pass


class Learner:
    def __init__(self, account):
        self.account = account
        self.http = HTTPConnection("127.0.0.1", 18090, timeout=4)
        self.cookie = None
        self.paper = None

    def call(self, method, path, body=None):
        headers = {"Content-Type": "application/json", "Origin": "http://localhost:8090"}
        if self.cookie:
            headers["Cookie"] = self.cookie
        self.http.request(method, path, json.dumps(body) if body is not None else None, headers)
        response = self.http.getresponse()
        raw = response.read()
        if response.status // 100 != 2:
            raise BadResponse(str(response.status))
        cookie = response.getheader("Set-Cookie")
        if cookie:
            self.cookie = cookie.split(";", 1)[0]
        return json.loads(raw) if raw else {}

    def logout(self):
        try:
            self.call("POST", "/api/auth/logout", {})
        finally:
            self.cookie = None

    def prepare(self, file):
        self.call("POST", "/api/auth/login", self.account)
        try:
            papers = self.call("GET", "/api/papers")["papers"]
            existing = next((p for p in papers if p["title"] == file["title"]), None)
            self.paper = (existing or self.call("POST", "/api/papers", file)["paper"])["id"]
        finally:
            self.logout()

    def journey(self):
        stats = {}
        stage = "login"
        begin = time.monotonic()
        path = "/api/papers/" + self.paper
        errors = []
        calls = 0
        try:
            calls += 1
            self.call("POST", "/api/auth/login", self.account)
            practice = time.monotonic()
            stage = "list"
            calls += 1
            if not any(p["id"] == self.paper for p in self.call("GET", "/api/papers")["papers"]):
                raise BadResponse("paper_missing")
            stage = "open"
            calls += 1
            revision = self.call("GET", path)["revision"]
            stage = "answer"
            start = time.monotonic()
            calls += 1
            answer = self.call("PUT", path + "/answers/q1", {"option_id": "a", "expected_revision": revision})
            stats["answer"] = (time.monotonic()-start)*1000
            if answer["answer"]["is_correct"] is not True:
                raise BadResponse("grade")
            stage = "next"
            calls += 1
            saved = self.call("PUT", path + "/position", {"question_id": "q2", "expected_revision": answer["revision"]})
            stage = "resume"
            calls += 1
            opened = self.call("GET", path)
            if opened["revision"] != saved["revision"] or opened["current_question_id"] != "q2" or not opened["questions"][0]["answer"]["is_correct"]:
                raise BadResponse("restore")
            end = time.monotonic()
            stats.update(workflow=(end-begin)*1000, practice=(end-practice)*1000)
        except (BadResponse, OSError, HTTPException, KeyError, ValueError) as e:
            errors.append(stage + "/" + (str(e) if isinstance(e, BadResponse) else type(e).__name__))
            self.http.close()
        finally:
            if self.cookie:
                calls += 1
                try:
                    self.logout()
                except (BadResponse, HTTPException, OSError) as e:
                    errors.append("logout/" + type(e).__name__)
                    self.http.close()
        return stats, errors, calls


SQL = """
SELECT json_build_object(
 'connections', (SELECT count(*) FROM pg_stat_activity WHERE backend_type='client backend' AND pid<>pg_backend_pid()),
 'max_connections',current_setting('max_connections')::int,
 'active',(SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND backend_type='client backend' AND pid<>pg_backend_pid() AND state='active'),
 'lock_waits',(SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock'),
 'deadlocks',deadlocks,'transactions',xact_commit+xact_rollback,
 'stats_reset',stats_reset,'started',pg_postmaster_start_time()) FROM pg_stat_database WHERE datname=current_database()
"""


def cpu_milli(value):
    for suffix, factor in [("n", 1e-6), ("u", 1e-3), ("m", 1)]:
        if value.endswith(suffix):
            return round(float(value[:-1])*factor, 2)
    return float(value)*1000


def observe():
    hpa = json.loads(kube("get", "hpa/app", "-o", "json"))["status"]
    app = json.loads(kube("get", "deploy/app", "-o", "json"))["status"]
    db = json.loads(kube("exec", "db-0", "--", "psql", "-XAt", "-U", "grace", "-c", SQL))
    pod = json.loads(kube("get", "--raw", "/apis/metrics.k8s.io/v1beta1/namespaces/grace/pods/db-0"))
    container = next(c for c in pod["containers"] if c["name"] == "postgres")
    return dict(available=app.get("availableReplicas", 0), desired=hpa.get("desiredReplicas"),
                db_cpu_m=cpu_milli(container["usage"]["cpu"]), db_cpu_at=pod["timestamp"],
                db_cpu_window=pod["window"], **db)


def summary(rows):
    timings = {}
    for key in ["workflow", "practice", "answer"]:
        values = sorted(r[1][key] for r in rows if key in r[1])
        timings[key+"_ms"] = {f"p{q}": round(values[math.ceil(len(values)*q/100)-1], 2)
                              for q in [50, 95, 99] if values}
    errors = Counter(e for r in rows for e in r[2])
    return dict(started=len(rows), failed=sum(bool(r[2]) for r in rows),
                calls=sum(r[3] for r in rows), errors=dict(errors), **timings)


def arm(learners, rate):
    # An account has at most one in-flight workflow. Record missed slots explicitly.
    start = time.monotonic()+1
    def worker(i):
        rows = []
        for slot in range(i, rate*SECONDS, len(learners)):
            delay = start+slot/rate-time.monotonic()
            if delay < -len(learners)/rate:
                continue
            time.sleep(max(0, delay))
            scheduled = slot / rate
            lag = max(0, time.monotonic() - start - scheduled)
            rows.append((scheduled, *learners[i].journey(), lag, time.monotonic() - start))
        return rows
    samples = []
    with ThreadPoolExecutor(max_workers=len(learners)) as pool:
        jobs = [pool.submit(worker, i) for i in range(len(learners))]
        for offset in range(0, SECONDS+1, 5):
            time.sleep(max(0, start+offset-time.monotonic()))
            samples.append(dict(t=offset, **observe()))
        rows = [r for job in jobs for r in job.result()]
    bins = [dict(t=t, **summary([r for r in rows if t<=r[0]<t+BIN])) for t in range(0, SECONDS, BIN)]
    print("completed",len(rows),"workflows; ready replicas",sorted({s["available"] for s in samples}),flush=True)
    planned = rate * SECONDS
    overall = summary(rows)
    elapsed = max([SECONDS] + [r[5] for r in rows])
    assert sum(b["started"] for b in bins) == overall["started"] <= planned
    lag = sorted(r[4] for r in rows)
    return dict(planned=planned, missed=planned-len(rows), overall=overall,
                completed_per_s=round((len(rows)-overall["failed"])/elapsed, 2),
                start_lag_p95_ms=round(1000*lag[math.ceil(.95*len(lag))-1], 2) if lag else None,
                last_minute=summary([r for r in rows if r[0]>=SECONDS-60]), bins=bins, samples=samples)


def wait_two():
    for _ in range(100):
        s = json.loads(kube("get", "deploy/app", "-o", "json"))["status"]
        if s.get("replicas")==2 and s.get("availableReplicas")==2:
            return
        time.sleep(2)
    raise RuntimeError("Two replicas did not become available")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--rate", type=int, choices=[15, 60, 120], default=15)
    args = parser.parse_args()
    out = args.output
    accounts = json.loads(Path(os.environ["GRACE_DEMO_ACCOUNTS"]).read_text())
    if sorted(a["username"] for a in accounts)!=[f"dashdemo{i}" for i in range(10)]:
        raise RuntimeError("Use the ten dedicated local rehearsal accounts")
    spec = json.loads(kube("get","hpa/app","-o","json"))["spec"]
    if (spec["minReplicas"], spec["maxReplicas"],spec["scaleTargetRef"]["name"])!=(2,5,"app"):
        raise RuntimeError("Unexpected HPA configuration")
    learners = [Learner(a) for a in accounts]
    file = json.loads((ROOT/"examples/french-basics.json").read_text())
    file["title"] = "Grace HPA comparison (local test fixture)"
    for user in learners:
        user.prepare(file)
        stats, errors, _ = user.journey()
        if errors or "practice" not in stats:
            raise RuntimeError(f"Workflow preflight failed: {errors}")
    observe()
    result = dict(rate=args.rate, seconds=SECONDS, accounts=len(learners), context="grace-dash-demo")
    print("Starting fixed-2 arm",flush=True)
    limit(5,2)
    try:
        wait_two()
        result["fixed"] = arm(learners, args.rate)
        out.write_text(json.dumps(result,indent=2)+"\n")
    finally:
        current=json.loads(kube("get","hpa/app","-o","json"))["spec"]["maxReplicas"]
        if current==2:
            limit(2,5)
    print("Starting HPA 2–5 arm",flush=True)
    result["hpa"] = arm(learners, args.rate)
    out.write_text(json.dumps(result,indent=2)+"\n")


if __name__ == "__main__":
    signal.signal(signal.SIGTERM,lambda *_: sys.exit(143))
    main()
