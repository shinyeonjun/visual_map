"""Remove AI co-author trailers from commit messages."""

from __future__ import annotations

import re
import sys
from pathlib import Path

STRIP_PATTERNS = (
    re.compile(r"^Co-[Aa]uthored-[Bb]y:\s*Cursor\s*<cursoragent@cursor\.com>\s*$"),
    re.compile(r"^Co-[Aa]uthored-[Bb]y:\s*Claude\b.*<noreply@anthropic\.com>\s*$"),
    re.compile(r"^Made-with:\s*Cursor\s*$"),
)


def strip_message(text: str) -> str:
    kept: list[str] = []
    for line in text.splitlines():
        if any(pattern.match(line.strip()) for pattern in STRIP_PATTERNS):
            continue
        kept.append(line)

    while kept and kept[-1] == "":
        kept.pop()

    if not kept:
        return ""

    return "\n".join(kept) + "\n"


def main() -> int:
    if len(sys.argv) > 1:
        path = Path(sys.argv[1])
        path.write_text(strip_message(path.read_text(encoding="utf-8")), encoding="utf-8")
        return 0

    sys.stdout.write(strip_message(sys.stdin.read()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
