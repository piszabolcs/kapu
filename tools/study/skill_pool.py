"""Collect every agent skill on this machine into one folder, one directory per
skill name, for the skill-finding measurement (never committed).

    python tools/study/skill_pool.py <out-dir>

Sources, first one wins for a name: the Claude desktop skills, the newest
installed version of every Claude Code plugin, the plugin marketplaces, the
skills synced from claude.ai, and F:/AI/A_and_I_Coding/claude-skills (the
user's own). A skill whose text holds a sensitive term of the portfolio
configuration is left out. Writes `<out-dir>/pool.json`: name, description,
source and size of every skill.
"""

import json
import re
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from collect_skills import DESKTOP_SKILLS, PLUGIN_CACHE, newest, sensitive_terms, split_front_matter  # noqa: E402

HOME = Path.home()
MARKETPLACES = HOME / ".claude/plugins/marketplaces"
SYNCED = HOME / ".claude/skills/synced"
OWN = Path("F:/AI/A_and_I_Coding/claude-skills")
SKIP = shutil.ignore_patterns("node_modules", ".git", "evals", "__pycache__", "*.pyc")


def candidates():
    """Skill directories in source order; newest copy first within a source."""
    desktop = sorted(DESKTOP_SKILLS.glob("*/*/skills/*/SKILL.md"), key=lambda p: -p.stat().st_mtime)
    yield from (("desktop", p.parent) for p in desktop)
    for plugin in (p for m in PLUGIN_CACHE.iterdir() if m.is_dir() for p in m.iterdir() if p.is_dir()):
        versions = [v for v in plugin.iterdir() if v.is_dir()]
        if versions:
            yield from (("plugin:" + plugin.name, p.parent) for p in newest(versions).rglob("SKILL.md"))
    yield from (("marketplace", p.parent) for p in MARKETPLACES.rglob("SKILL.md"))
    yield from (("synced", p.parent) for p in SYNCED.rglob("SKILL.md"))
    yield from (("own", p.parent) for p in OWN.rglob("SKILL.md"))


def main(out: Path):
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    terms = sensitive_terms()
    pool, skipped = {}, []
    for source, directory in candidates():
        try:
            text = (directory / "SKILL.md").read_text(encoding="utf-8").replace("\r\n", "\n")
        except UnicodeDecodeError:
            skipped.append(f"{directory} (not UTF-8)")
            continue
        meta, _ = split_front_matter(text)
        name = re.sub(r"[^a-z0-9-]+", "-", (meta.get("name") or directory.name).lower()).strip("-")
        if name in pool:
            continue
        everything = " ".join(p.read_text(encoding="utf-8", errors="ignore").lower() for p in directory.rglob("*.md"))
        if any(t in everything for t in terms):
            skipped.append(f"{directory} (sensitive term)")
            continue
        shutil.copytree(directory, out / name, ignore=SKIP)
        pool[name] = {"description": meta.get("description", ""), "source": source,
                      "skill_md_bytes": len(text.encode("utf-8"))}
    (out / "pool.json").write_text(json.dumps(pool, indent=1, ensure_ascii=False), encoding="utf-8")
    print(f"{len(pool)} skills in {out}; skipped {len(skipped)}")
    for s in skipped:
        print("  skipped", s)


if __name__ == "__main__":
    main(Path(sys.argv[1]))
