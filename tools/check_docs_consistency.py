import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
README = ROOT / "README.md"
DOCS_README = ROOT / "docs" / "README.md"


def extract_readme_badge_count(text: str) -> int:
    match = re.search(r"tests-(\d+)%20passing", text)
    if not match:
        raise ValueError("Could not find test badge count in README.md")
    return int(match.group(1))


def extract_readme_summary_count(text: str) -> int:
    match = re.search(r"\*\*(\d+) Tests Passing\*\*", text)
    if not match:
        raise ValueError("Could not find 'Tests Passing' summary count in README.md")
    return int(match.group(1))


def extract_docs_summary_count(text: str) -> int:
    match = re.search(r"\b(\d+) tests passing across all components\b", text, re.IGNORECASE)
    if not match:
        raise ValueError("Could not find docs test summary count in docs/README.md")
    return int(match.group(1))


def main() -> int:
    readme_text = README.read_text(encoding="utf-8")
    docs_text = DOCS_README.read_text(encoding="utf-8")

    badge = extract_readme_badge_count(readme_text)
    readme_summary = extract_readme_summary_count(readme_text)
    docs_summary = extract_docs_summary_count(docs_text)

    if not (badge == readme_summary == docs_summary):
        print("Documentation test-count drift detected:")
        print(f"  README badge count:        {badge}")
        print(f"  README summary count:      {readme_summary}")
        print(f"  docs/README summary count: {docs_summary}")
        print("\nPlease update documentation counts so they match.")
        return 1

    print(f"Docs consistency check passed (count={badge}).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
