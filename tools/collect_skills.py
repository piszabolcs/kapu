"""Collect the Claude skills installed on this machine into one Markdown file per
room, for a local test corpus (bench/korpusz/, never committed).

    python tools/collect_skills.py bench/korpusz/rooms

Sources: the Claude desktop skills (AppData/Roaming/Claude/.../skills-plugin),
the newest installed version of every Claude Code plugin (~/.claude/plugins/cache)
and the locally uploaded marketplace (~/.claude/plugins/marketplaces/local-desktop-app-uploads).
Every SKILL.md is a room `sk-<plugin>-<skill>`; every other Markdown file of a
skill (references, guides) is a room of its own. The YAML front matter becomes
the title, description and tags lines of the room template (FR-05).

Files that contain a term from SENSITIVE (company names a portfolio must never
show) are left out and listed.
"""

import json
import re
import sys
import unicodedata
from pathlib import Path

HOME = Path.home()
DESKTOP_SKILLS = HOME / "AppData/Roaming/Claude/local-agent-mode-sessions/skills-plugin"
PLUGIN_CACHE = HOME / ".claude/plugins/cache"
UPLOADS = HOME / ".claude/plugins/marketplaces/local-desktop-app-uploads"
PORTFOLIO_CONFIG = UPLOADS / "github-portfolio-publisher/skills/github-portfolio-publisher/assets/portfolio.config.json"
SKIP_DIRS = {"scripts", "assets", "templates", "node_modules", ".git", "evals", "tests"}
MAX_STEM = 60


def slug(text):
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def sensitive_terms():
    try:
        return [t.lower() for t in json.loads(PORTFOLIO_CONFIG.read_text(encoding="utf-8"))["sensitive_terms"]]
    except (OSError, KeyError, ValueError):
        return []


def newest(dirs):
    return max(dirs, key=lambda d: d.stat().st_mtime)


def skill_dirs():
    """(plugin name, skill dir) for every skill, newest copy of each."""
    found = {}
    # Claude desktop skills: several session folders may hold copies.
    for skill_md in DESKTOP_SKILLS.glob("*/*/skills/*/SKILL.md"):
        key = ("anthropic-skills", skill_md.parent.name)
        found.setdefault(key, []).append(skill_md.parent)
    # Claude Code plugins: only the newest installed version of each plugin.
    for plugin in (p for m in PLUGIN_CACHE.iterdir() if m.is_dir() for p in m.iterdir() if p.is_dir()):
        versions = [v for v in plugin.iterdir() if v.is_dir()]
        if not versions:
            continue
        for skill_md in newest(versions).rglob("SKILL.md"):
            found.setdefault((plugin.name, skill_md.parent.name), []).append(skill_md.parent)
    for skill_md in UPLOADS.rglob("SKILL.md"):
        plugin = skill_md.relative_to(UPLOADS).parts[0]
        found.setdefault((plugin, skill_md.parent.name), []).append(skill_md.parent)
    return {key: newest(dirs) for key, dirs in found.items()}


def split_front_matter(text):
    if text.startswith("---\n"):
        end = text.find("\n---", 4)
        if end != -1:
            meta = {}
            for line in text[4:end].splitlines():
                m = re.match(r"^([A-Za-z_-]+):\s*(.*)$", line)
                if m:
                    meta[m.group(1).lower()] = m.group(2).strip().strip("\"'")
            return meta, text[end + 4:].lstrip("\n")
    return {}, text


def first_heading(body):
    m = re.search(r"^# (.+)$", body, re.M)
    return m.group(1).strip() if m else None


def drop_first_h1(body):
    lines = body.splitlines()
    for i, line in enumerate(lines[:5]):
        if line.startswith("# "):
            return "\n".join(lines[:i] + lines[i + 1:]).lstrip("\n")
    return body


def short(text, limit=300):
    text = re.sub(r"\s+", " ", text).strip()
    return text if len(text) <= limit else text[: limit - 1].rsplit(" ", 1)[0] + "…"


def room(title, description, tags, body):
    return f"# {title}\n> {short(description)}\ntags: {', '.join(tags)}\n\n{body.rstrip()}\n"


def stem(*parts):
    s = slug("-".join(parts))
    return s[:MAX_STEM].rstrip("-")


def main():
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "bench/korpusz/rooms")
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("sk-*.md"):
        old.unlink()
    terms = sensitive_terms()
    written, skipped, used = [], [], set()

    def write(name, text, source):
        low = text.lower()
        hit = next((t for t in terms if t in low), None)
        if hit:
            skipped.append(f"{source} (sensitive term)")
            return
        base, n = name, 2
        while name in used:
            name = f"{base[: MAX_STEM - 3]}-{n}"
            n += 1
        used.add(name)
        (out / f"{name}.md").write_text(text, encoding="utf-8", newline="\n")
        written.append(name)

    for (plugin, skill), directory in sorted(skill_dirs().items()):
        try:
            meta, body = split_front_matter((directory / "SKILL.md").read_text(encoding="utf-8").replace("\r\n", "\n"))
        except UnicodeDecodeError:
            skipped.append(f"{directory / 'SKILL.md'} (not UTF-8)")
            continue
        name = meta.get("name") or skill
        title = first_heading(body) or name
        description = meta.get("description") or title
        write(stem("sk", plugin, skill), room(title, description, [plugin, name, "skill"], drop_first_h1(body)), directory / "SKILL.md")
        for doc in sorted(directory.rglob("*.md")):
            rel = doc.relative_to(directory)
            if doc.name in {"SKILL.md", "LICENSE.md"} or SKIP_DIRS & set(rel.parts[:-1]):
                continue
            try:
                text = doc.read_text(encoding="utf-8").replace("\r\n", "\n")
            except UnicodeDecodeError:
                skipped.append(f"{doc} (not UTF-8)")
                continue
            _, doc_body = split_front_matter(text)
            doc_title = first_heading(doc_body) or doc.stem.replace("-", " ").replace("_", " ")
            doc_desc = f"Reference of the {name} skill ({plugin}): {doc_title}."
            write(
                stem("sk", plugin, skill, *rel.with_suffix("").parts),
                room(doc_title, doc_desc, [plugin, name, "reference"], drop_first_h1(doc_body)),
                doc,
            )
    print(f"{len(written)} rooms written to {out}, {len(skipped)} files left out")
    for s in skipped:
        print("  left out:", s)


if __name__ == "__main__":
    main()
