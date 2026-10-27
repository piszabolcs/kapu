"""Fixed dev/test split of the question sets (plan v2 M1).

    python tools/study/split_questions.py <study-data> <bench/splits>

A question is in the dev part when the first 8 hex digits of
sha256("kapu-v2-split|" + question) / 2^32 is below 0.4, otherwise in the test
part: about 40% and 60%, independent of the line order, and the same on every
machine. The Hungarian questions translated to English (`q-hu-en`) take the
part of their Hungarian line, so a question and its translation never sit on
two sides. Only the hashes go to the repository (the questions come from
Stack Overflow and other people's documentation).
"""

import hashlib
import json
import sys
from pathlib import Path

SALT = "kapu-v2-split|"
DEV_SHARE = 0.4
SETS = {
    "s": "questions-s.jsonl",
    "m": "questions-m.jsonl",
    "l": "questions-l.jsonl",
    "en": "en-questions.jsonl",
    "hu": "q-hu.jsonl",
    "plc": "plc/questions/answerable.jsonl",
}
PAIRED = {"hu-en": ("q-hu-en.jsonl", "hu")}


def qhash(text):
    return hashlib.sha256((SALT + text).encode("utf-8")).hexdigest()


def part(h):
    return "dev" if int(h[:8], 16) / 2**32 < DEV_SHARE else "test"


def questions(path):
    return [json.loads(line)["kerdes"] for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def main():
    data, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    parts = {}
    for name, file in SETS.items():
        hashes = [qhash(q) for q in questions(data / file)]
        parts[name] = [part(h) for h in hashes]
        write(out / f"{name}.json", name, file, hashes, parts[name], f"sha256({SALT!r} + question)")
    for name, (file, base) in PAIRED.items():
        qs = questions(data / file)
        assert len(qs) == len(parts[base]), name
        hashes = [qhash(q) for q in qs]
        write(out / f"{name}.json", name, file, hashes, parts[base], f"the part of line i of `{base}`")


def write(path, name, file, hashes, parts, rule):
    doc = {
        "set": name,
        "questions_file": file,
        "rule": rule,
        "dev_share": DEV_SHARE,
        "dev": parts.count("dev"),
        "test": parts.count("test"),
        # one entry per line of the questions file, in order: hash prefix and part
        "lines": [[h[:12], p] for h, p in zip(hashes, parts)],
    }
    path.write_text(json.dumps(doc, indent=1) + "\n", encoding="utf-8", newline="\n")
    print(f"{name}: {doc['dev']} dev, {doc['test']} test")


if __name__ == "__main__":
    main()
