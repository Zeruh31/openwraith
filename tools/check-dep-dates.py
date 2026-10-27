#!/usr/bin/env python3
"""Read-only audit of Cargo.lock against the dependency cutoff.

Lists every crates.io package of Cargo.lock whose locked version was
released after the cutoff date, with the newest version of the same
semver-compatible line released on or before it, and prints the
`cargo update --precise` commands that pin them down.

It only reads Cargo.lock and queries the crates.io API (one request per
crate name, at most one per second, with a generic User-Agent). Results are
cached in a JSON file so that a second run is fast.

    python tools/check-dep-dates.py [--cutoff 2026-10-13] [--cache FILE]

Exit code 0 when nothing is late, 1 otherwise.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import tomllib
import urllib.request
from pathlib import Path

USER_AGENT = "openwraith-dep-audit"
API = "https://crates.io/api/v1/crates/{name}/versions"


def fetch_versions(name: str) -> list[dict]:
    """Every published version of `name` (num, created_at, yanked)."""
    versions: list[dict] = []
    query = "?per_page=100"
    while query:
        request = urllib.request.Request(
            API.format(name=name) + query, headers={"User-Agent": USER_AGENT}
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            data = json.load(response)
        versions.extend(
            {"num": v["num"], "created_at": v["created_at"], "yanked": v["yanked"]}
            for v in data.get("versions", [])
        )
        time.sleep(1.0)
        query = (data.get("meta") or {}).get("next_page")
    return versions


def parse(version: str) -> tuple:
    """A sortable key of a semver string (pre-releases sort first; build
    metadata such as `+spec-1.1.0` is ignored)."""
    core, _, pre = version.split("+")[0].partition("-")
    numbers = tuple(int(p) if p.isdigit() else 0 for p in core.split("."))
    return numbers + ((0, pre) if pre else (1, ""),)


def is_prerelease(version: str) -> bool:
    """Whether `version` is a pre-release (`-alpha`, not `+spec-...`)."""
    return "-" in version.split("+")[0]


def compatible(a: str, b: str) -> bool:
    """Whether `b` is in the caret range of `a` (same major, or same minor for 0.x)."""
    pa, pb = parse(a), parse(b)
    if pa[0] != 0:
        return pa[0] == pb[0]
    if pa[1] != 0:
        return pa[:2] == pb[:2]
    return pa[:3] == pb[:3]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--lock", default=str(Path(__file__).resolve().parent.parent / "Cargo.lock"))
    parser.add_argument("--cutoff", default="2026-10-13")
    parser.add_argument("--cache", default=str(Path.home() / ".cache" / "openwraith-dep-audit.json"))
    args = parser.parse_args()

    cutoff = f"{args.cutoff}T23:59:59.999999Z"
    lock = tomllib.loads(Path(args.lock).read_text(encoding="utf-8"))
    packages = [
        p for p in lock.get("package", [])
        if p.get("source", "").startswith("registry+https://github.com/rust-lang/crates.io-index")
        or p.get("source", "").startswith("sparse+https://index.crates.io")
    ]
    cache_path = Path(args.cache)
    cache: dict[str, list[dict]] = {}
    if cache_path.exists():
        cache = json.loads(cache_path.read_text(encoding="utf-8"))

    late = []
    for package in sorted(packages, key=lambda p: (p["name"], parse(p["version"]))):
        name, version = package["name"], package["version"]
        if name not in cache:
            try:
                cache[name] = fetch_versions(name)
            except Exception as error:  # network problems are reported, not fatal
                print(f"? {name} {version}: {error}", file=sys.stderr)
                continue
            cache_path.parent.mkdir(parents=True, exist_ok=True)
            cache_path.write_text(json.dumps(cache), encoding="utf-8")
        released = {v["num"]: v["created_at"] for v in cache[name]}
        created = released.get(version)
        if created is None or created <= cutoff:
            continue
        candidates = [
            v["num"] for v in cache[name]
            if not v["yanked"] and v["created_at"] <= cutoff and compatible(version, v["num"])
            and not is_prerelease(v["num"])
        ]
        best = max(candidates, key=parse) if candidates else None
        late.append((name, version, created[:10], best))

    for name, version, created, best in late:
        hint = f"cargo update -p {name}@{version} --precise {best}" if best else "no compatible release before the cutoff"
        print(f"{name} {version} (released {created}): {hint}")
    if not late:
        print(f"every crates.io package of {args.lock} was released on or before {args.cutoff}")
    return 1 if late else 0


if __name__ == "__main__":
    sys.exit(main())
