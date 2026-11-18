"""Run from the presenter's computer. Python 3.10+, standard library only."""
import argparse
import concurrent.futures
import json
import os
import secrets
import socket
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

INGEST = "http://127.0.0.1:9091/demo"


def millis():
    return time.time_ns() // 1_000_000


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args):
        return None


def send(url, data=None, headers=None, timeout=5):
    body = None if data is None else json.dumps(data).encode()
    req = urllib.request.Request(url, body, {"Content-Type": "application/json", **(headers or {})})
    # Never forward credentials across redirects or through an ambient HTTP proxy.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    try:
        with opener.open(req, timeout=timeout) as res:
            return res.status, res.headers
    except urllib.error.HTTPError as error:
        return error.code, error.headers


def publish(packet):
    for attempt in range(3):
        try:
            status, _ = send(INGEST, packet)
            if status == 204:
                return
            raise RuntimeError(f"dashboard returned {status}")
        except OSError:
            if attempt == 2:
                raise
            time.sleep(0.25)


def accounts():
    with open(os.environ["GRACE_DEMO_ACCOUNTS"], encoding="utf-8") as source:
        creds = json.load(source)
    if (not isinstance(creds, list) or len(creds) != 10
        or not all(isinstance(row, dict) and all(isinstance(row.get(k), str) and row[k]
                    for k in ("username", "password")) for row in creds)
        or len({row["username"] for row in creds}) != 10):
        raise ValueError("expected 10 distinct accounts with username and password")
    return [{k: row[k] for k in ("username", "password")} for row in creds]


def login(base, account, request_id=None):
    headers = {"Origin": base}
    if request_id:
        headers["X-Grace-Demo-Id"] = request_id
    return send(base + "/api/auth/login", account, headers)


def logout(base, headers):
    cookie = headers["Set-Cookie"].split(";", 1)[0]
    status, _ = send(base + "/api/auth/logout", {}, {"Origin": base, "Cookie": cookie})
    if not 200 <= status < 300:
        raise RuntimeError(f"logout returned {status}")


def burst(base, wrong):
    creds = accounts()
    if wrong:
        creds[-1]["password"] = secrets.token_hex(24)
    ids = [secrets.token_hex(16) for _ in creds]
    publish({"type": "run", "ids": ids, "started": millis()})
    barrier = threading.Barrier(len(ids))
    def one(i):
        barrier.wait(timeout=10)
        start, tick = millis(), time.monotonic_ns()
        status, timeout, headers = None, False, None
        try:
            status, headers = login(base, creds[i], ids[i])
        except (TimeoutError, urllib.error.URLError, OSError) as exc:
            timeout = isinstance(getattr(exc, "reason", exc), (TimeoutError, socket.timeout))
        end = start + (time.monotonic_ns() - tick) // 1_000_000
        try:
            publish(dict(type="result", id=ids[i], start=start, end=end, status=status, timeout=timeout))
        finally:
            if status == 200:
                logout(base, headers)
        return status
    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as pool:
        results = list(pool.map(one, range(10)))
    print(json.dumps({"sent": 10, "statuses": results}))
    return all(s == (401 if wrong and i == 9 else 200) for i, s in enumerate(results))


def probe(base):
    while True:
        start = time.monotonic()
        try:
            status, _ = send(base + "/health/ready")
            ok = status == 200
        except OSError:
            ok = False
        try:
            publish(dict(type="probe", time=millis(), ms=round((time.monotonic() - start) * 1000), ok=ok))
        except (OSError, RuntimeError):
            print("Dashboard ingest unavailable; retrying on the next probe.", file=sys.stderr)
        time.sleep(10)


def scale(base, seconds, pause):
    # Use the real password check. Close every successful session and cap the run.
    creds = accounts()
    stop = threading.Event()
    deadline = time.monotonic() + seconds
    def worker(i):
        logins = 0
        while not stop.is_set() and time.monotonic() < deadline:
            try:
                status, headers = login(base, creds[i])
                if status != 200:
                    raise RuntimeError(f"login returned {status}")
                logout(base, headers)
                logins += 1
            except Exception:
                stop.set()
                raise
            stop.wait(pause)
        return logins
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        jobs = [pool.submit(worker, i) for i in range(2)]
        try:
            while not all(j.done() for j in jobs):
                time.sleep(0.2)
        finally:
            stop.set()
        print("Completed login/logout pairs:", sum(j.result() for j in jobs))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["burst", "probe", "scale"])
    parser.add_argument("--url", required=True, help="Grace origin")
    parser.add_argument("--wrong-password", action="store_true", help="fail the last request in a burst")
    parser.add_argument("--seconds", type=int, choices=range(1, 181), metavar="1..180", default=120)
    parser.add_argument("--pause", type=float, default=0.25, help="seconds between login/logout pairs per worker")
    args = parser.parse_args()
    base = args.url.rstrip("/")
    url = urllib.parse.urlsplit(base)
    if url.path or url.query or url.fragment or url.username or not url.hostname or not (
            url.scheme == "https" or url.scheme == "http" and url.hostname in ("localhost", "127.0.0.1")):
        parser.error("use the HTTPS Grace origin, or HTTP on localhost for minikube")
    if not 0.1 <= args.pause <= 10:
        parser.error("--pause must be between 0.1 and 10 seconds")
    try:
        if args.mode == "burst":
            sys.exit(0 if burst(base, args.wrong_password) else 1)
        elif args.mode == "scale":
            scale(base, args.seconds, args.pause)
        else:
            probe(base)
    except KeyboardInterrupt:
        pass
