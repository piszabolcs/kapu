"""Collects the study-pilot measurements into one committed results file.

    python tools/study/collect_results.py <study-data dir> <out.json>

Only numbers, ids and source commits are written; no third-party text.
"""

import json
import subprocess
import sys
from pathlib import Path

SYSTEMS = {"c0": "closed book", "c2b": "agentic chunk RAG", "c3": "grep/read on the Markdown folder", "c4": "llms.txt", "c5": "KAPU MCP"}


def load(p: Path):
    return json.loads(p.read_text(encoding="utf-8")) if p.is_file() else None


def commit(repo: Path):
    try:
        return subprocess.run(["git", "-C", str(repo), "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


def judge_scores(pilot: Path, judge: str):
    key = load(pilot / "judge-key.json")
    grades = {}
    for f in sorted(pilot.glob(f"judge-{judge}-*.json")):
        for q, cands in load(f).items():
            for letter, v in cands.items():
                grades[(q, key[q][letter])] = v["correct"].strip().lower()
    out = {}
    for s in SYSTEMS:
        vals = [g for (q, sys_), g in grades.items() if sys_ == s]
        out[s] = {"yes": vals.count("yes"), "partly": vals.count("partly"), "no": vals.count("no"), "n": len(vals)}
    return out, grades


def main(data: Path, out: Path):
    pilot = data / "pilot"
    sonnet, gs = judge_scores(pilot, "sonnet")
    haiku, gh = judge_scores(pilot, "haiku")
    common = set(gs) & set(gh)
    sizes = {}
    for size, kind in (("s", "rust-book"), ("m", "k8s"), ("l", "mdn")):
        man = load(data / f"rooms-{size}.meta" / "manifest.json") or {}
        qs = [json.loads(l) for l in (data / f"questions-{size}-full.jsonl").read_text(encoding="utf-8").splitlines() if l.strip()] if (data / f"questions-{size}-full.jsonl").is_file() else []
        sizes[size] = {
            "kind": kind,
            "rooms": len(man),
            "questions": len(qs),
            "question_ids": [q["qid"] for q in qs],
            "gold": [q["gold"] for q in qs],
            "scale": load(data / f"scale-{size}.json"),
            "scale_experimental_line_limit_64mib": load(data / f"scale-{size}-xl.json"),
            "kapu_bench": load(data / f"bench-{size}.json"),
            "chunk_baselines": load(data / f"chunk-{size}.json"),
        }
    result = {
        "date": "2026-10-23",
        "specification": "v1.0.3 (bench room-level metric D130)",
        "sources": {
            "rust-book": {"repo": "rust-lang/book", "commit": commit(data / "src-rust-book"), "license": "MIT OR Apache-2.0"},
            "k8s": {"repo": "kubernetes/website (content/en/docs)", "commit": commit(data / "src-k8s"), "license": "CC BY 4.0"},
            "mdn": {"repo": "mdn/content (files/en-us)", "commit": commit(data / "src-mdn"), "license": "CC BY-SA 2.5"},
            "questions": "Stack Overflow API 2.3: top-voted questions with an accepted answer that links exactly one page of the corpus (CC BY-SA); ids only",
        },
        "sizes": sizes,
        "agentic_pilot_s": {
            "questions": [q["qid"] for q in load(pilot / "questions.json")],
            "model": "Claude Sonnet subagents, 10 questions per agent (closed book: 30 in one agent)",
            "systems": SYSTEMS,
            "judge_sonnet": sonnet,
            "judge_haiku": haiku,
            "judge_agreement": round(sum(gs[k] == gh[k] for k in common) / len(common), 3) if common else None,
            "tokens": load(pilot / "token-stats.json"),
        },
        "format_comparison_pixellab": {
            "content": "the user's PixelLab knowledge file (206,899 tokens; third-party, not published), the same 12 amateur requests as REPORT chapter 10",
            "formats": {"kapu": "KAPU MCP (REPORT chapter 10 run)", "md": "one Markdown file with grep and line reads", "pdf": "the same text printed to a 219-page PDF, page reads (max 20) and a reader-style search"},
            "judge": "three blind Claude Sonnet judges, claims checked against the documentation",
            "full_file_in_context_tokens_per_request": 206899,
            "result": load(data / "pixellab" / "compare-result.json"),
        },
    }
    out.write_text(json.dumps(result, indent=1, ensure_ascii=False), encoding="utf-8")
    print("written", out)


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
