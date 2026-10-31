#!/usr/bin/env python3
"""Download the real-world Bun executables in manifest.toml into out/.

The corpus is third-party release assets, pinned by tag and sha256, so that
answer keys written against them (module counts, addon export names) stay
true until someone changes the manifest on purpose. Refreshing the corpus is
editing the manifest; ``--check-latest`` reports which projects have a newer
release and downloads nothing.

These binaries are only ever read by pao, never run. Bun runtimes, which the
fixture builds in tools/bun-matrix do run, are downloaded separately and by
hand for that reason, and do not belong in this manifest.

Usage, from anywhere:
    tools/corpus/fetch.py                 # fetch what is missing, verify all
    tools/corpus/fetch.py terminaltype    # only entries with these names
    tools/corpus/fetch.py --check-latest  # report newer releases upstream

Each asset lands at out/<name>/<asset>. A .zip or .tar.gz is also unpacked
beside it, into out/<name>/<asset without extension>/. An entry with no
sha256 is downloaded and its digest printed for the manifest; the run then
fails, so an unpinned file is never mistaken for a verified one.

Exit status is 0 only if every requested entry is present and verified.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tarfile
import tomllib
import urllib.error
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
MANIFEST = HERE / "manifest.toml"
OUT = HERE / "out"

# GitHub serves public release assets and the releases API without auth; the
# API allows 60 unauthenticated requests an hour, which --check-latest stays
# well under with one request per repository.
DOWNLOAD_URL = "https://github.com/{repo}/releases/download/{tag}/{asset}"
LATEST_URL = "https://api.github.com/repos/{repo}/releases/latest"
USER_AGENT = "pao-corpus-fetch"

# Read size for streaming a download through the hash. Arbitrary; assets are
# 60-210 MB, and anything from 64 KiB up keeps this I/O-bound.
CHUNK = 1 << 20


@dataclass(frozen=True)
class Entry:
    """One release asset, as manifest.toml pins it."""

    name: str
    repo: str
    tag: str
    asset: str
    sha256: str | None
    why: str

    @property
    def url(self) -> str:
        return DOWNLOAD_URL.format(repo=self.repo, tag=self.tag, asset=self.asset)

    @property
    def path(self) -> Path:
        return OUT / self.name / self.asset


class FetchError(Exception):
    """An entry that could not be fetched or verified; the run continues."""


def load_manifest() -> list[Entry]:
    with MANIFEST.open("rb") as file:
        data = tomllib.load(file)
    entries = []
    seen: set[str] = set()
    for raw in data.get("asset", []):
        try:
            entry = Entry(
                name=raw["name"],
                repo=raw["repo"],
                tag=raw["tag"],
                asset=raw["asset"],
                sha256=raw.get("sha256"),
                why=raw["why"],
            )
        except KeyError as error:
            sys.exit(
                f"{MANIFEST}: an [[asset]] is missing {error}; see the header comment"
            )
        # Names are directories under out/, so two entries must not share one.
        if entry.name in seen:
            sys.exit(
                f"{MANIFEST}: two entries are named {entry.name!r}; names must be unique"
            )
        seen.add(entry.name)
        entries.append(entry)
    return entries


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        while chunk := file.read(CHUNK):
            digest.update(chunk)
    return digest.hexdigest()


def download(entry: Entry) -> None:
    """Writes the asset to entry.path, through a .part file, so an
    interrupted download is never taken for a complete one."""
    entry.path.parent.mkdir(parents=True, exist_ok=True)
    part = entry.path.with_name(entry.path.name + ".part")
    request = urllib.request.Request(entry.url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request) as response, part.open("wb") as file:
            while chunk := response.read(CHUNK):
                file.write(chunk)
    except urllib.error.HTTPError as error:
        part.unlink(missing_ok=True)
        if error.code == 404:
            raise FetchError(
                f"gone upstream: {entry.url} returns 404; pin another tag or drop the entry"
            ) from error
        raise FetchError(f"{entry.url}: HTTP {error.code}") from error
    except urllib.error.URLError as error:
        part.unlink(missing_ok=True)
        raise FetchError(f"{entry.url}: {error.reason}") from error
    part.replace(entry.path)


def unpack(entry: Entry) -> None:
    """Unpacks a .zip or .tar.gz beside the asset, refusing any member that
    would land outside the target directory. The archives are third-party,
    so their member names are not trusted."""
    asset = entry.path
    if asset.name.endswith(".zip"):
        target = asset.with_name(asset.name.removesuffix(".zip"))
        if target.exists():
            return
        with zipfile.ZipFile(asset) as archive:
            for member in archive.namelist():
                resolved = (target / member).resolve()
                if not resolved.is_relative_to(target.resolve()):
                    raise FetchError(
                        f"{asset.name}: member {member!r} escapes the archive"
                    )
            archive.extractall(target)
    elif asset.name.endswith(".tar.gz"):
        target = asset.with_name(asset.name.removesuffix(".tar.gz"))
        if target.exists():
            return
        with tarfile.open(asset) as archive:
            # The "data" filter rejects absolute paths, `..`, links out of the
            # target and device files (PEP 706).
            archive.extractall(target, filter="data")


def fetch(entry: Entry) -> None:
    if not entry.path.exists():
        print(f"{entry.name}: downloading {entry.url}", flush=True)
        download(entry)
    actual = sha256_of(entry.path)
    if entry.sha256 is None:
        raise FetchError(
            f"not pinned; add to its manifest entry:\n    sha256 = {actual!r}"
        )
    if actual != entry.sha256:
        # Kept on disk so it can be inspected; the next run re-hashes it and
        # fails the same way until it is deleted or the manifest changes.
        raise FetchError(
            f"sha256 mismatch: {entry.path} is {actual}, manifest pins {entry.sha256}; "
            "delete the file to download it again"
        )
    unpack(entry)
    print(f"{entry.name}: ok")


def check_latest(entries: list[Entry]) -> int:
    """Prints each repository whose latest release is not the pinned tag."""
    status = 0
    for repo in sorted({entry.repo for entry in entries}):
        pinned = sorted({entry.tag for entry in entries if entry.repo == repo})
        request = urllib.request.Request(
            LATEST_URL.format(repo=repo),
            headers={"User-Agent": USER_AGENT, "Accept": "application/vnd.github+json"},
        )
        try:
            with urllib.request.urlopen(request) as response:
                latest = json.load(response)["tag_name"]
        except (urllib.error.URLError, KeyError, ValueError) as error:
            print(f"{repo}: cannot read the latest release: {error}")
            status = 1
            continue
        state = "up to date" if pinned == [latest] else f"latest is {latest}"
        print(f"{repo}: pinned {', '.join(pinned)}, {state}")
    return status


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("names", nargs="*", help="fetch only these entries")
    parser.add_argument(
        "--check-latest",
        action="store_true",
        help="report projects with a release newer than the pinned tag; download nothing",
    )
    args = parser.parse_args()

    entries = load_manifest()
    if args.names:
        unknown = set(args.names) - {entry.name for entry in entries}
        if unknown:
            parser.error(f"not in the manifest: {', '.join(sorted(unknown))}")
        entries = [entry for entry in entries if entry.name in args.names]

    if args.check_latest:
        return check_latest(entries)

    failures = 0
    for entry in entries:
        try:
            fetch(entry)
        except FetchError as error:
            print(f"{entry.name}: {error}", file=sys.stderr)
            failures += 1
    if failures:
        print(f"{failures} of {len(entries)} entries failed", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
