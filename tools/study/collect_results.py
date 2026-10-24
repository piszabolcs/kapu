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


def kapu_split():
    """KAPU tokens of the PixelLab run per request (p1..p12) and tool, from the local call log."""
    log = Path(__file__).resolve().parents[2] / "bench" / "korpusz" / "mcp-calls.jsonl"
    if not log.is_file():
        return None
    out = {f"p{i}": {"kapu_map": 0, "kapu_search": 0, "kapu_open": 0} for i in range(1, 13)}
    for line in log.read_text(encoding="utf-8").splitlines():
        r = json.loads(line)
        if r.get("tag") in out and "pixellab" in json.dumps(r.get("args")) and r["tool"] in out[r["tag"]]:
            out[r["tag"]][r["tool"]] += r["tokens"]
    return out


def bench_seconds(log: Path):
    """`kapu bench` run time per size from the re-measurement log (`== bench s` ... `in N s`)."""
    if not log.is_file():
        return None
    out, size = {}, None
    for line in log.read_text(encoding="utf-8").splitlines():
        if line.startswith("== bench "):
            size = line.split()[2]
        elif size and " in " in line and line.rstrip().endswith(" s"):
            out[size] = int(line.rsplit(" in ", 1)[1].split()[0])
            size = None
    return out


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
            "content": "the user's PixelLab knowledge file (206,899 tokens; third-party, not published), the same 12 amateur requests as the live run of 2026-10-23",
            "formats": {"kapu": "KAPU MCP (live run of 2026-10-23)", "md": "one Markdown file with grep and line reads", "pdf": "the same text printed to a 219-page PDF, page reads (max 20) and a reader-style search"},
            "judge": "three blind Claude Sonnet judges, claims checked against the documentation",
            "full_file_in_context_tokens_per_request": 206899,
            "result": load(data / "pixellab" / "compare-result.json"),
            "kapu_tokens_by_tool_per_request": kapu_split(),
        },
        "remeasure_v104": {
            "specification": "v1.0.4 (D131-D138)",
            "scale": {size: load(data / f"scale-{size}-v104.json") for size in ("s", "m")},
            "scale_l_experimental": load(data / "scale-l-xl-v104.json"),
            "kapu_bench": {size: load(data / f"bench-{size}-v104.json") for size in ("m", "l")},
            "agents_s_kapu": (load(pilot / "token-stats.json") or {}).get("c5v"),
            "agents_s_ab_judge": "KAPU v1.0.3 30/30, KAPU v1.0.4 30/30 (one blind Sonnet A/B judge)",
            "pixellab_same_prompt": load(data / "pixellab" / "compare-result-round2.json"),
        },
        "remeasure_v11": {
            "specification": "v1.1, file format 1.1 (D139-D141)",
            "date": "2026-10-24",
            "scale": {size: load(data / f"scale-{size}-v11final.json") for size in ("s", "m", "l")},
            "kapu_bench": {size: load(data / f"bench-{size}-v11final.json") for size in ("s", "m", "l")},
            "kapu_bench_seconds": bench_seconds(data / "remeasure_v11.log"),
        },
    }
    out.write_text(json.dumps(result, indent=1, ensure_ascii=False), encoding="utf-8")
    print("written", out)


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
