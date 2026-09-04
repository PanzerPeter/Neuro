#!/usr/bin/env python3
"""Garbage-collect stale Cargo build artifacts in `target/`.

Cargo never removes the artifacts of previous builds: every code change mints a
new metadata hash and leaves the old `deps/<crate>-<hash>.*` files behind
forever. In this workspace a single test binary statically links LLVM/MLIR and
weighs well over a hundred megabytes, so a few months of ordinary iteration
turns `target/` into tens of thousands of dead files and a hundred-plus
gigabytes on disk.

This script groups every file in `<profile>/deps` into build *units* keyed by
`<crate name>-<hash>`, keeps the newest `--keep` units per crate name (plus
anything touched within `--grace-days`), and deletes the rest along with their
matching `.fingerprint/` and `build/` directories.

Deleting a live artifact is not a correctness hazard: Cargo simply rebuilds
whatever is missing on the next invocation.

Usage:
    python tools/clean_stale_target.py --dry-run
    python tools/clean_stale_target.py            # keep newest unit per crate
    python tools/clean_stale_target.py --keep 3 --grace-days 3
    python tools/clean_stale_target.py --max-gb 20
"""

from __future__ import annotations

import argparse
import re
import shutil
import sys
import time
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path

# `libfoo_bar-1a2b3c4d5e6f7a8b.rlib`, `foo_bar-1a2b3c4d5e6f7a8b`,
# `foo_bar-1a2b3c4d5e6f7a8b.foo_bar.9f8e-cgu.0.rcgu.dwo`, ...
UNIT_RE = re.compile(r"^(?:lib)?(?P<name>.+?)-(?P<hash>[0-9a-f]{8,17})(?P<rest>[.-].*)?$")

GIB = 1024**3


@dataclass
class Unit:
    """One `<crate>-<hash>` build unit and every file that belongs to it."""

    name: str
    key: str
    paths: list[Path] = field(default_factory=list)
    size: int = 0
    mtime: float = 0.0

    def add(self, path: Path, size: int, mtime: float) -> None:
        self.paths.append(path)
        self.size += size
        self.mtime = max(self.mtime, mtime)


def parse_unit(filename: str) -> tuple[str, str] | None:
    match = UNIT_RE.match(filename)
    if match is None:
        return None
    return match.group("name"), f"{match.group('name')}-{match.group('hash')}"


def collect(deps: Path) -> dict[str, Unit]:
    units: dict[str, Unit] = {}
    for entry in deps.iterdir():
        parsed = parse_unit(entry.name)
        if parsed is None:
            continue
        name, key = parsed
        try:
            stat = entry.stat()
        except OSError:
            continue
        size = stat.st_size
        if entry.is_dir():
            size = sum(f.stat().st_size for f in entry.rglob("*") if f.is_file())
        units.setdefault(key, Unit(name=name, key=key)).add(entry, size, stat.st_mtime)
    return units


def attach_siblings(units: dict[str, Unit], profile: Path) -> None:
    """Pull each unit's `.fingerprint/` and `build/` directories into its group."""
    for subdir in (".fingerprint", "build"):
        directory = profile / subdir
        if not directory.is_dir():
            continue
        for entry in directory.iterdir():
            parsed = parse_unit(entry.name)
            if parsed is None:
                continue
            unit = units.get(parsed[1])
            if unit is None:
                continue
            try:
                size = sum(f.stat().st_size for f in entry.rglob("*") if f.is_file())
                unit.add(entry, size, entry.stat().st_mtime)
            except OSError:
                continue


def select_stale(units: dict[str, Unit], keep: int, grace_days: float) -> list[Unit]:
    cutoff = time.time() - grace_days * 86400
    by_name: dict[str, list[Unit]] = defaultdict(list)
    for unit in units.values():
        by_name[unit.name].append(unit)

    stale: list[Unit] = []
    for group in by_name.values():
        group.sort(key=lambda u: u.mtime, reverse=True)
        for unit in group[keep:]:
            if unit.mtime < cutoff:
                stale.append(unit)
    return stale


def trim_to_budget(units: dict[str, Unit], stale: list[Unit], max_gb: float) -> list[Unit]:
    """Extend `stale` with the oldest surviving units until the budget is met."""
    budget = max_gb * GIB
    doomed = {unit.key for unit in stale}
    survivors = sorted(
        (u for u in units.values() if u.key not in doomed), key=lambda u: u.mtime
    )
    remaining = sum(u.size for u in survivors)
    for unit in survivors:
        if remaining <= budget:
            break
        stale.append(unit)
        remaining -= unit.size
    return stale


def remove(path: Path) -> None:
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path, ignore_errors=True)
    else:
        path.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default="target", help="target directory (default: target)")
    parser.add_argument(
        "--keep", type=int, default=1, help="newest build units to keep per crate (default: 1)"
    )
    parser.add_argument(
        "--grace-days",
        type=float,
        default=1.0,
        help="never delete units touched within this many days (default: 1)",
    )
    parser.add_argument(
        "--max-gb",
        type=float,
        default=None,
        help="after the per-crate pass, keep deleting oldest units until under this size",
    )
    parser.add_argument("--dry-run", action="store_true", help="report only, delete nothing")
    args = parser.parse_args()

    target = Path(args.target).resolve()
    if not target.is_dir():
        print(f"no target directory at {target}", file=sys.stderr)
        return 1

    total_freed = 0
    for profile in sorted(p for p in target.iterdir() if (p / "deps").is_dir()):
        units = collect(profile / "deps")
        attach_siblings(units, profile)
        stale = select_stale(units, args.keep, args.grace_days)
        if args.max_gb is not None:
            stale = trim_to_budget(units, stale, args.max_gb)

        freed = sum(unit.size for unit in stale)
        files = sum(len(unit.paths) for unit in stale)
        print(
            f"{profile.name}: {len(units)} units, "
            f"{len(stale)} stale ({files} paths, {freed / GIB:.1f} GiB)"
        )
        if not args.dry_run:
            for unit in stale:
                for path in unit.paths:
                    remove(path)
        total_freed += freed

    verb = "would free" if args.dry_run else "freed"
    print(f"{verb} {total_freed / GIB:.1f} GiB")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
