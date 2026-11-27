"""Compare the public homepage and its frontend assets with the tested image."""

import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit
from urllib.request import urlopen


class Assets(HTMLParser):
    def __init__(self):
        super().__init__()
        self.paths = set()

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "script" and attrs.get("src"):
            self.paths.add(attrs["src"])
        if tag == "link" and attrs.get("rel") in {"stylesheet", "modulepreload"}:
            self.paths.add(attrs["href"])


def check_site(base, directory):
    def check(path, expected):
        with urlopen(base.rstrip("/") + path, timeout=15) as response:
            if response.status != 200 or response.read() != expected:
                raise ValueError(f"{path} does not match the tested image")

    for path in ("/health/live", "/health/ready"):
        with urlopen(base.rstrip("/") + path, timeout=15) as response:
            if response.status != 200:
                raise ValueError(f"{path} returned {response.status}")
    index = (directory / "index.html").read_bytes()
    check("/", index)
    assets = Assets()
    assets.feed(index.decode())
    if not assets.paths:
        raise ValueError("No built frontend assets in index.html")
    for asset in sorted(assets.paths):
        url = urlsplit(asset)
        if url.netloc or url.scheme or not url.path.startswith("/") or ".." in url.path.split("/"):
            raise ValueError(f"Expected a local asset path: {asset}")
        check(asset, (directory / url.path.lstrip("/")).read_bytes())
    print(f"Health, homepage and {len(assets.paths)} frontend assets match.")


if __name__ == "__main__":
    check_site(sys.argv[1], Path(sys.argv[2]))
