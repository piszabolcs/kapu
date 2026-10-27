"""How often agents open a door from the neighbour list (plan v2 N2a).

    python tools/study/neighbour_follow.py <kapu-mcp> <out.json> <log.jsonl>=<file.kapu> ...

The call logs of the live runs (`calllog.py`: one JSON line per tool call,
`tag` = one run of one question) hold the arguments, not the replies. So every
`kapu_open` is asked again from `kapu-mcp` on the same file and its neighbour
block (D126: at most 8, `link` first, then `term`, then `sim`) is read from
the reply. A later `kapu_open` in the same run follows a neighbour when one of
its ids is in the neighbour block of an earlier open (`next`: of the open just
before it). Counted per edge type, with the price of the neighbour blocks.
The replies come from today's server, so a run made with an older version may
have seen a slightly different list; ids the file no longer has are counted
as missing.
"""

import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from measure_v12 import Server  # noqa: E402

import tiktoken  # noqa: E402

ENC = tiktoken.get_encoding("o200k_base")
LINE = re.compile(r"^  (\S+) \| .* \| \d+t \| (\w+)$")


def neighbour_block(text):
    """[(id, type)] of every neighbour block in an open reply, and its price."""
    out, price, inside, block = [], 0, False, []
    for line in text.splitlines():
        if line == "neighbours (data, not a suggestion):":
            inside, block = True, [line]
            continue
        if inside:
            m = LINE.match(line)
            if m:
                out.append((m.group(1), m.group(2)))
                block.append(line)
                continue
            if line.startswith("  +"):
                block.append(line)
                continue
            inside = False
            price += len(ENC.encode("\n".join(block) + "\n"))
    if inside:
        price += len(ENC.encode("\n".join(block) + "\n"))
    return out, price


def main():
    exe, out = sys.argv[1], Path(sys.argv[2])
    result = {"logs": {}}
    total = Counter()
    for spec in sys.argv[3:]:
        log, kfile = spec.split("=", 1)
        server = Server(exe)
        cache = {}

        def neighbours(door, kfile=kfile, server=server, cache=cache):
            if door not in cache:
                text, _, _ = server.call("kapu_open", {"file": kfile, "ids": [door]})
                cache[door] = None if "E010" in text[:300] else neighbour_block(text)
            return cache[door]

        runs = defaultdict(list)
        for line in Path(log).read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            if row["tool"] == "kapu_open" and not row.get("error"):
                runs[row["tag"]].append(row["args"].get("ids", []))
        c = Counter()
        for opens in runs.values():
            seen = {}  # neighbour id -> type, of every earlier open
            last = {}
            for i, ids in enumerate(opens):
                c["open_calls"] += 1
                c["doors_opened"] += len(ids)
                if i > 0:
                    c["later_open_calls"] += 1
                    followed = [seen[d] for d in ids if d in seen]
                    if followed:
                        c["later_calls_following_a_neighbour"] += 1
                    if any(d in last for d in ids):
                        c["later_calls_following_the_previous_open"] += 1
                    for kind in followed:
                        c[f"followed_{kind}"] += 1
                last = {}
                for d in ids:
                    got = neighbours(d)
                    if got is None:
                        c["missing_ids"] += 1
                        continue
                    shown, price = got
                    c["neighbour_tokens"] += price
                    c["neighbours_shown"] += len(shown)
                    for n, kind in shown:
                        c[f"shown_{kind}"] += 1
                        seen.setdefault(n, kind)
                        last.setdefault(n, kind)
        server.close()
        c["runs"] = len(runs)
        result["logs"][Path(log).name] = {"file": Path(kfile).name, **c}
        total.update(c)
    if total["later_open_calls"]:
        total_rate = total["later_calls_following_a_neighbour"] / total["later_open_calls"]
    else:
        total_rate = None
    result["total"] = dict(total)
    result["follow_rate"] = total_rate
    if total["doors_opened"] - total["missing_ids"]:
        result["neighbour_tokens_per_door"] = total["neighbour_tokens"] / (total["doors_opened"] - total["missing_ids"])
    out.write_text(json.dumps(result, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=1))


if __name__ == "__main__":
    main()
