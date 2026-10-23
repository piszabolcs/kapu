"""THIRD-PARTY.md: every third-party crate in the shipped binaries, with its
licence, plus the vector model and tokenizer (M9, spec chapter 13).

    tools/.venv/Scripts/python tools/third_party.py THIRD-PARTY.md

Uses `cargo metadata` and follows only normal (not dev or build-only)
dependencies of `kapu-cli` and `kapu-mcp`, for the host platform.
"""
import json
import subprocess
import sys

ROOTS = ("kapu-cli", "kapu-mcp")


def main() -> None:
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", host()],
            check=True,
            capture_output=True,
            text=True,
            encoding="utf-8",
        ).stdout
    )
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    workspace = set(meta["workspace_members"])
    seen, stack = set(), [pid for pid in workspace if packages[pid]["name"] in ROOTS]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    rows = sorted(
        (packages[p]["name"], packages[p]["version"], packages[p].get("license") or "?", packages[p].get("repository") or "")
        for p in seen
        if p not in workspace
    )
    # with a path argument the file is written directly, LF only (FR-02 style)
    out = open(sys.argv[1], "w", encoding="utf-8", newline="\n") if len(sys.argv) > 1 else sys.stdout
    out.write("# Third-party components\n\n")
    out.write("KAPU code is MIT (`LICENSE`), the specification CC BY 4.0. The binaries `kapu` and `kapu-mcp` contain the crates below; every licence is on the allowlist of `deny.toml` (checked by `cargo deny` in CI).\n\n")
    out.write("## Vector model\n\n")
    out.write("| Component | Licence | Source |\n|---|---|---|\n")
    out.write("| potion-multilingual-128M (Model2Vec static embeddings; shipped as its int8 form, `tools/quantize.py`) | MIT | https://huggingface.co/minishlab/potion-multilingual-128M |\n")
    out.write("| tokenizer of BAAI/bge-m3 (`tokenizer.json`, distributed with the model) | MIT | https://huggingface.co/BAAI/bge-m3 |\n\n")
    out.write(f"## Rust crates ({len(rows)})\n\n| Crate | Version | Licence | Repository |\n|---|---|---|---|\n")
    for name, version, lic, repo in rows:
        out.write(f"| {name} | {version} | {lic} | {repo} |\n")


def host() -> str:
    info = subprocess.run(["rustc", "-vV"], check=True, capture_output=True, text=True).stdout
    return next(l.split(": ", 1)[1] for l in info.splitlines() if l.startswith("host: "))


if __name__ == "__main__":
    main()
