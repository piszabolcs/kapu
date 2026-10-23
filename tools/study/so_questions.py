"""Real questions with a gold page from Stack Overflow (study pilot, Task 3).

    python tools/study/so_questions.py <size> <kind> <tags> <pages> <study-data dir>
    e.g.  so_questions.py s rust-book rust 20 ../study-data

Fetches top-voted questions of each tag that have an accepted answer
(Stack Exchange API 2.3, no key, responses cached under so-cache/), keeps a
question when its accepted answer links exactly one distinct page of the
corpus (urlmap of prepare_corpus.py), and writes questions-<size>.jsonl
(`kapu bench` format, the title as the question) and questions-<size>-full.jsonl
(id, title, body text, accepted answer text, gold id). The posts are CC BY-SA
and stay in the study-data folder.
"""

import gzip
import hashlib
import html
import json
import re
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from prepare_corpus import url_key  # noqa: E402

API = "https://api.stackexchange.com/2.3"


def get(path: str, params: dict, cache: Path):
    params = {**params, "site": "stackoverflow"}
    url = f"{API}/{path}?{urllib.parse.urlencode(params)}"
    key = cache / (hashlib.sha1(url.encode()).hexdigest() + ".json")
    if key.is_file():
        return json.loads(key.read_text(encoding="utf-8"))
    req = urllib.request.Request(url, headers={"Accept-Encoding": "gzip", "User-Agent": "kapu-study-pilot"})
    with urllib.request.urlopen(req, timeout=60) as r:
        raw = r.read()
        if r.headers.get("Content-Encoding") == "gzip":
            raw = gzip.decompress(raw)
    data = json.loads(raw)
    cache.mkdir(parents=True, exist_ok=True)
    key.write_text(json.dumps(data), encoding="utf-8")
    time.sleep(max(0.25, data.get("backoff", 0)))
    return data


def text_of(body_html: str) -> str:
    t = re.sub(r"<pre[^>]*>", "\n```\n", body_html)
    t = re.sub(r"</pre>", "\n```\n", t)
    t = re.sub(r"<(p|br|li|h\d)[^>]*>", "\n", t)
    t = re.sub(r"<[^>]+>", "", t)
    return re.sub(r"\n{3,}", "\n\n", html.unescape(t)).strip()


def main(size, kind, tags, pages, data):
    data = Path(data)
    cache = data / "so-cache"
    urlmap = json.loads((data / f"rooms-{size}.meta" / "urlmap.json").read_text(encoding="utf-8"))
    questions = {}
    for tag in tags.split(","):
        for page in range(1, int(pages) + 1):
            d = get("questions", {"order": "desc", "sort": "votes", "tagged": tag, "filter": "withbody", "pagesize": 100, "page": page}, cache)
            for q in d.get("items", []):
                if q.get("accepted_answer_id"):
                    questions.setdefault(q["question_id"], q)
            if not d.get("items"):
                break
            if d.get("quota_remaining", 1) < 10:
                print("quota low, stopping", file=sys.stderr)
                break
    ids = sorted({q["accepted_answer_id"] for q in questions.values()})
    answers = {}
    for i in range(0, len(ids), 100):
        d = get(f"answers/{';'.join(map(str, ids[i:i + 100]))}", {"filter": "withbody", "pagesize": 100}, cache)
        for a in d.get("items", []):
            answers[a["answer_id"]] = a
    kept = []
    for q in questions.values():
        a = answers.get(q["accepted_answer_id"])
        if not a:
            continue
        golds = {urlmap.get(url_key(kind, h) or "") for h in re.findall(r'href="([^"]+)"', a["body"])}
        golds.discard(None)
        if len(golds) == 1:
            kept.append((q, a, golds.pop()))
    kept.sort(key=lambda x: -x[0]["score"])
    with (data / f"questions-{size}.jsonl").open("w", encoding="utf-8", newline="\n") as bench, \
            (data / f"questions-{size}-full.jsonl").open("w", encoding="utf-8", newline="\n") as full:
        for q, a, gold in kept:
            title = html.unescape(q["title"])
            bench.write(json.dumps({"kerdes": title, "valasz_fajl": f"{gold}.md", "valasz_szakasz": "", "nyelv": "en"}, ensure_ascii=False) + "\n")
            full.write(json.dumps({"qid": q["question_id"], "aid": a["answer_id"], "score": q["score"], "tags": q["tags"], "title": title,
                                   "body": text_of(q["body"]), "answer": text_of(a["body"]), "gold": gold}, ensure_ascii=False) + "\n")
    print(json.dumps({"size": size, "questions_with_accepted": len(questions), "kept_single_gold": len(kept), "distinct_gold_rooms": len({k[2] for k in kept})}))


if __name__ == "__main__":
    main(*sys.argv[1:6])
