"""Markdown tables of plan v2 for bench/RESULTS.md, from the results file.

    python tools/study/v2_tables.py bench/results/2026-10-27-plan-v2.json
"""

import json
import sys
from pathlib import Path

NAMES = {"s": "Rust book", "m": "Kubernetes", "l": "MDN", "en": "Skills, English", "hu-en": "Skills, translated",
         "hu": "Skills, Hungarian", "plc": "PLC manuals"}
CONFIGS = {"base": "KAPU 0.2.1", "collapse-1": "KAPU, collapsed (0.3.0)", "chunk_bm25": "Chunks, BM25",
           "chunk_vector": "Chunks, vectors", "chunk_hybrid": "Chunks, hybrid"}


def ci(d, lo, hi):
    return f"{d:+.3f} [{lo:+.3f}, {hi:+.3f}]"


def main():
    r = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    for title, key in (("Phase 2, against 0.2.1 (m0)", "fixes"), ("On top of the collapse (m1)", "fixes_m1"),
                       ("J4b with J4c on top of the collapse, six sets without MDN (m2)", "fixes_m2")):
        if key not in r:
            continue
        print(f"#### {title}\n")
        print("| Fix | Variants | Chosen on dev | Test Δ room recall@10 [95%] | Test Δ room MRR@10 [95%] | Worst set | Kept |")
        print("|---|---:|---|---:|---:|---:|---|")
        for f in r[key]:
            print(f"| {f['fix']} | {f['variants']} | {f['variant']} | {ci(f['d_r10'], f['lo_r10'], f['hi_r10'])} | "
                  f"{ci(f['d_mrr'], f['lo_mrr'], f['hi_mrr'])} | {f['worst_set']:+.3f} | {'yes' if f['accepted'] else 'no'} |")
        print()
    print("#### Retrieval, test part, room level\n")
    print("| Set | Questions | Ranking | recall@1 | recall@10 | recall@100 | MRR@10 | nDCG@10 |")
    print("|---|---:|---|---:|---:|---:|---:|---:|")
    for row in r["retrieval"]:
        if row["config"] not in CONFIGS:
            continue
        m = row["hybrid_room"]
        print(f"| {NAMES.get(row['set'], row['set'])} | {row['n']} | {CONFIGS[row['config']]} | {m['r@1']:.3f} | {m['r@10']:.3f} | "
              f"{m['r@100']:.3f} | {m['mrr@10']:.3f} | {m['ndcg@10']:.3f} |")
    print("\n#### Why a question is missed (M3), test part\n")
    print("| Set | Ranking | Missed | Duplication | Fusion | Rescoring | Ranking 11-100 | Vocabulary | Room hit, door missed |")
    print("|---|---|---:|---:|---:|---:|---:|---:|---:|")
    for e in r["errors"]:
        print(f"| {NAMES.get(e['set'], e['set'])} | {CONFIGS.get(e['config'], e['config'])} | {e['misses']} | {e['duplication']} | "
              f"{e['fusion']} | {e['rescore']} | {e['ranking']} | {e['vocabulary']} | {e['merge (room hit, door missed)']} |")
    if "ask" in r:
        print("\n#### kapu_ask modes (J12), test part\n")
        print("| Set | Mode | Answered | Reply tokens (median) | Tokens without the page, per question | Search hits in 600 tokens |")
        print("|---|---|---:|---:|---:|---:|")
        for s, modes in r["ask"]["sets"].items():
            for mode, parts in modes.items():
                t = parts["test"]
                print(f"| {NAMES.get(s, s)} | {mode} | {t['answered']:.3f} | {t['tokens_median']:.0f} | "
                      f"{t['wasted_tokens_per_ask']:.0f} | {t['search_hits_mean']:.1f} |")


if __name__ == "__main__":
    main()
