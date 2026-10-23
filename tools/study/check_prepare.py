"""Checks of prepare_corpus.py on the prepared corpora (study pilot, Task 1).

    python tools/study/check_prepare.py <study-data dir>
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from prepare_corpus import slug, url_key  # noqa: E402


def lookup(meta: Path, kind: str, url: str):
    urlmap = json.loads((meta / "urlmap.json").read_text(encoding="utf-8"))
    return urlmap.get(url_key(kind, url) or "")


def main(data: Path):
    assert slug("ch04-01-what-is-ownership") == "ch04-01-what-is-ownership"
    long = slug("a" * 63 + "-bcdef")
    assert len(long) <= 64 and not long.endswith("-"), long
    assert slug("Árvíztűrő tükörfúrógép") == "arvizturo-tukorfurogep"

    s = data / "rooms-s.meta"
    own = "https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html"
    assert lookup(s, "rust-book", own + "#the-stack-and-the-heap") == "ch04-01-what-is-ownership"
    assert lookup(s, "rust-book", "https://doc.rust-lang.org/stable/book/second-edition/ch04-01-what-is-ownership.html") == "ch04-01-what-is-ownership"
    assert lookup(s, "rust-book", "https://doc.rust-lang.org/std/vec/struct.Vec.html") is None

    m = data / "rooms-m.meta"
    assert lookup(m, "k8s", "https://kubernetes.io/docs/concepts/workloads/pods/#pod-lifecycle") == "concepts-workloads-pods"
    assert lookup(m, "k8s", "https://kubernetes.io/docs/concepts/workloads/pods") == "concepts-workloads-pods"

    l_meta = data / "rooms-l.meta"
    if l_meta.is_dir():
        a = lookup(l_meta, "mdn", "https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/map")
        b = lookup(l_meta, "mdn", "https://developer.mozilla.org/en-us/docs/web/javascript/reference/global_objects/array/map#syntax")
        assert a and a == b, (a, b)
    print("check_prepare: ok")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
