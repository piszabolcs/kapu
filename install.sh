#!/bin/sh
# Install KAPU (kapu, kapu-mcp, kapu-gui and the embedding model) from the
# latest GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/piszabolcs/kapu/main/install.sh | sh
#
# Environment: KAPU_VERSION (a tag such as v0.3.0; default: the latest
# release), KAPU_HOME (where the release is unpacked; default
# ~/.local/share/kapu), KAPU_BIN (where the commands go; default ~/.local/bin),
# KAPU_BASE_URL (a mirror of the release downloads; default GitHub).
# The archive's SHA-256 is checked against the .sha256 file of the release.
set -eu

REPO="piszabolcs/kapu"
HOME_DIR="${KAPU_HOME:-$HOME/.local/share/kapu}"
BIN_DIR="${KAPU_BIN:-$HOME/.local/bin}"
BASE="${KAPU_BASE_URL:-https://github.com/$REPO/releases/download}"

fail() { printf 'kapu install: %s\n' "$1" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "needs $1"; }
need curl
need tar
need uname

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target="linux-x64" ;;
  Darwin-arm64) target="macos-arm64" ;;
  *) fail "no prebuilt archive for $(uname -s) $(uname -m); build from source: https://github.com/$REPO#try-it-in-30-seconds" ;;
esac

tag="${KAPU_VERSION:-}"
if [ -z "$tag" ]; then
  tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
  [ -n "$tag" ] || fail "could not find the latest release of $REPO"
fi
version="${tag#v}"
name="kapu-$version-$target"
url="$BASE/$tag/$name.tar.gz"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
printf 'Downloading %s\n' "$url"
curl -fsSL -o "$tmp/$name.tar.gz" "$url" || fail "download failed: $url"
curl -fsSL -o "$tmp/$name.tar.gz.sha256" "$url.sha256" || fail "no checksum file: $url.sha256"

want=$(cut -d ' ' -f 1 "$tmp/$name.tar.gz.sha256")
if command -v sha256sum >/dev/null 2>&1; then
  have=$(sha256sum "$tmp/$name.tar.gz" | cut -d ' ' -f 1)
else
  have=$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d ' ' -f 1)
fi
[ "$want" = "$have" ] || fail "checksum mismatch for $name.tar.gz"

mkdir -p "$HOME_DIR" "$BIN_DIR"
tar -xzf "$tmp/$name.tar.gz" -C "$HOME_DIR"
rm -f "$HOME_DIR/current"
ln -s "$HOME_DIR/$name" "$HOME_DIR/current"

# small launchers, so each program finds the model next to its real path
for prog in kapu kapu-mcp kapu-gui; do
  [ -x "$HOME_DIR/$name/$prog" ] || continue
  printf '#!/bin/sh\nexec "%s/current/%s" "$@"\n' "$HOME_DIR" "$prog" > "$BIN_DIR/$prog"
  chmod +x "$BIN_DIR/$prog"
done

printf '\nInstalled KAPU %s in %s\n' "$version" "$HOME_DIR/$name"
"$BIN_DIR/kapu" --version | head -n 1
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) printf '\nAdd %s to your PATH, for example:\n  export PATH="%s:$PATH"\n' "$BIN_DIR" "$BIN_DIR" ;;
esac
printf '\nNext:\n'
printf '  kapu map %s/current/examples/kapu-docs.kapu\n' "$HOME_DIR"
printf '  kapu import --skills ~/.claude/skills skills.kapu\n'
printf '  claude mcp add kapu -- %s/kapu-mcp\n' "$BIN_DIR"
