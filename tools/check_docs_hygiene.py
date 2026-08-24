"""Documentation hygiene gate.

Three classes of documentation rot are cheap to introduce and expensive to
notice, so they are enforced mechanically instead of by review:

1. Hand-maintained counts (test totals) drift the moment a test is added.
   The CI badge is the only honest source, so no count may be written down.
2. The workspace version is declared once in Cargo.toml. Any prose copy of it
   is stale as soon as the version is bumped.
3. Local-only paths (assistant configuration, private working notes, generated
   caches) are gitignored, so a committed file referencing one points nowhere
   for everyone but the author.

Run: python tools/check_docs_hygiene.py
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

TEXT_SUFFIXES = {".md", ".rs", ".nr", ".toml", ".yml", ".yaml", ".json", ".py", ".sh"}

# CHANGELOG.md is an append-only record: past entries legitimately mention old
# versions, and rewriting history to satisfy a linter would be worse than the
# rot it prevents. LICENSE files cite their own numbered clauses with §.
EXEMPT = {
    "CHANGELOG.md",
    "LICENSE",
    "CONTRIBUTING.md",
    "neuro-language-support/LICENSE",
    "tools/check_docs_hygiene.py",
}

TEST_COUNT_PATTERNS = [
    re.compile(r"tests-\d+%20passing"),
    re.compile(r"\b\d{2,}\s+[Tt]ests\s+[Pp]assing\b"),
    re.compile(r"\b\d{2,}\s+tests\s+pass\b", re.IGNORECASE),
    # Any written-down total, however it is phrased: "78 tests", "green at 806
    # tests", "11 slice unit tests". Each was true on the day it was typed. Two
    # digits minimum, so prose about an indexed arm ("arm 0 tests the tag") is
    # not swept up with it.
    re.compile(r"\b\d{2,}\s+(?:\w+\s+){0,2}tests\b", re.IGNORECASE),
]

# Gitignored or otherwise local-only paths. A committed file must not cite them.
PRIVATE_PATH_PATTERNS = [
    re.compile(r"(?<![\w/.])\.idea/"),
    re.compile(r"(?<![\w/.])\.claude/"),
    re.compile(r"(?<![\w/.])graphify-out/"),
    re.compile(r"(?<![\w/.-])CLAUDE\.md"),
]

# Internal specification section markers, e.g. "(§3.12)".
SPEC_MARKER = re.compile(r"§\s?\d")


def workspace_version() -> str:
    match = re.search(
        r"^\[workspace\.package\](?:.|\n)*?^version\s*=\s*\"([^\"]+)\"",
        (ROOT / "Cargo.toml").read_text(encoding="utf-8"),
        re.MULTILINE,
    )
    if not match:
        raise SystemExit("Could not read [workspace.package].version from Cargo.toml")
    return match.group(1)


def tracked_text_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout
    return [
        path
        for path in out.splitlines()
        if Path(path).suffix in TEXT_SUFFIXES and path not in EXEMPT
    ]


def main() -> int:
    version_pattern = re.compile(r"v?" + re.escape(workspace_version()) + r"\b")
    problems: list[str] = []

    for path in tracked_text_files():
        try:
            text = (ROOT / path).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        is_markdown = path.endswith(".md")

        for lineno, line in enumerate(text.splitlines(), start=1):
            where = f"{path}:{lineno}"
            for pattern in TEST_COUNT_PATTERNS:
                if pattern.search(line):
                    problems.append(
                        f"{where}: hard-coded test count — the CI badge is the source of truth"
                    )
            if is_markdown and version_pattern.search(line):
                problems.append(
                    f"{where}: hard-coded workspace version — cite Cargo.toml instead"
                )
            for pattern in PRIVATE_PATH_PATTERNS:
                if pattern.search(line):
                    problems.append(
                        f"{where}: reference to a local-only path — it does not exist for other readers"
                    )
            if SPEC_MARKER.search(line):
                problems.append(
                    f"{where}: internal spec section marker — describe the feature in plain terms"
                )

    if problems:
        print("Documentation hygiene violations:\n")
        for problem in problems:
            print(f"  {problem}")
        print(f"\n{len(problems)} violation(s).")
        return 1

    print("Docs hygiene check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
