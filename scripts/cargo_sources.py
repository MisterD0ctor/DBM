#!/usr/bin/env python3
"""Keep the Flatpak's list of Rust crates in step with Cargo.lock.

    scripts/cargo_sources.py write    regenerate packaging/flatpak/cargo-sources.json
    scripts/cargo_sources.py check    fail, naming the crates, if it is stale

The Flatpak compiles offline, so flatpak-builder fetches every crate first,
from this list. A crate Cargo.lock has and the list doesn't fails the build
deep inside cargo; `check` runs early in CI to fail with the crate's name.

The file is laid out exactly as flatpak-cargo-generator.py, from
flatpak-builder-tools, lays it out for crates from crates.io. That is every
crate the player uses; a git dependency is refused here and needs that tool.
"""

import json
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
LOCK = REPO / "Cargo.lock"
SOURCES = REPO / "packaging" / "flatpak" / "cargo-sources.json"
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"

# Points cargo at the unpacked crates instead of the network.
CONFIG = (
    '[source.vendored-sources]\ndirectory = "cargo/vendor"\n\n'
    '[source.crates-io]\nreplace-with = "vendored-sources"\n'
)


class SourcesError(Exception):
    pass


def render():
    with LOCK.open("rb") as f:
        lock = tomllib.load(f)
    sources = []
    for package in lock["package"]:
        source = package.get("source")
        # The workspace's own crates have no source: they are the checkout.
        if source is None:
            continue
        name, version = package["name"], package["version"]
        if source != CRATES_IO:
            raise SourcesError(
                f"{name} {version} is not from crates.io; list it with "
                "flatpak-cargo-generator.py instead"
            )
        checksum = package["checksum"]
        dest = f"cargo/vendor/{name}-{version}"
        sources.append({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
            "sha256": checksum,
            "dest": dest,
        })
        sources.append({
            "type": "inline",
            "contents": json.dumps({"package": checksum, "files": {}}),
            "dest": dest,
            "dest-filename": ".cargo-checksum.json",
        })
    sources.append({
        "type": "inline",
        "contents": CONFIG,
        "dest": "cargo",
        "dest-filename": "config",
    })
    return json.dumps(sources, indent=4)


def crates(text):
    return {
        source["dest"].removeprefix("cargo/vendor/")
        for source in json.loads(text)
        if source["type"] == "archive"
    }


def write():
    SOURCES.write_text(render(), encoding="utf-8")
    print(f"wrote {SOURCES.relative_to(REPO)}")


def check():
    wanted = render()
    current = SOURCES.read_text(encoding="utf-8") if SOURCES.exists() else "[]"
    if current == wanted:
        print("the Flatpak's crates are in step with Cargo.lock")
        return
    print(
        f"{SOURCES.relative_to(REPO)} is stale: run scripts/cargo_sources.py write",
        file=sys.stderr,
    )
    have, want = crates(current), crates(wanted)
    for crate in sorted(want - have):
        print(f"  missing  {crate}", file=sys.stderr)
    for crate in sorted(have - want):
        print(f"  unused   {crate}", file=sys.stderr)
    sys.exit(1)


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    try:
        if command == "write":
            write()
        elif command == "check":
            check()
        else:
            print(__doc__.strip())
            sys.exit(2)
    except SourcesError as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
