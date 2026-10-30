#!/usr/bin/env bash
# Installs dist, which builds the release (.github/workflows/release.yml),
# from its own GitHub release, each archive checked against the SHA-256
# pinned here before it is unpacked (docs/security.md, "CI and releases").
# It replaces dist's `curl | sh` installer, which is checked against
# nothing. The version is `cargo-dist-version` in dist-workspace.toml;
# changing it means changing the version and every digest here, taken from
# the `.sha256` files beside dist's archives and the digests GitHub shows
# for them, which must agree.
set -euo pipefail

version=0.33.1

case "${RUNNER_OS:-}-${RUNNER_ARCH:-}" in
  Linux-X64) target=x86_64-unknown-linux-gnu ext=tar.xz sha=da68e159d5fed1ee61b00284da444e719927dbd0b7d0e2c34e57a04414453cf9 ;;
  Linux-ARM64) target=aarch64-unknown-linux-gnu ext=tar.xz sha=e883f4c6c17f2458d6ced950d978c04b45423ed3a5fdbebcb8f8de568389e1c1 ;;
  macOS-X64) target=x86_64-apple-darwin ext=tar.xz sha=e9dc303035fbb2f44295948e0a076f1d1d218a0328e9b84e16955863bba92bf4 ;;
  macOS-ARM64) target=aarch64-apple-darwin ext=tar.xz sha=b8bf041a38e9e54ecf41eee2574ca6644043796620bb0ee8340e9c0acbaaae8f ;;
  Windows-X64) target=x86_64-pc-windows-msvc ext=zip sha=79dfcce84e6a8e69210f11fc23d445c3aac7af6a29e0e981867f053c0a869316 ;;
  *)
    echo "::error::No pinned dist $version for runner ${RUNNER_OS:-?}-${RUNNER_ARCH:-?}."
    exit 1
    ;;
esac

work="${RUNNER_TEMP:?}/dist-install"
rm -rf "$work"
mkdir -p "$work/unpacked"
archive="$work/cargo-dist-$target.$ext"
curl --proto '=https' --tlsv1.2 -fsSL -o "$archive" \
  "https://github.com/axodotdev/cargo-dist/releases/download/v$version/cargo-dist-$target.$ext"

# Hashed from standard input: Git Bash's sha256sum escapes a Windows path
# and puts a backslash before the hash.
if command -v sha256sum > /dev/null 2>&1; then
  actual="$(sha256sum < "$archive" | cut -d' ' -f1)"
else
  actual="$(shasum -a 256 < "$archive" | cut -d' ' -f1)"
fi
if [ "$actual" != "$sha" ]; then
  echo "::error::dist $version for $target has SHA-256 $actual, not the pinned $sha."
  exit 1
fi

if [ "$ext" = zip ]; then
  pwsh -NoProfile -Command "Expand-Archive -LiteralPath '$(cygpath -w "$archive")' -DestinationPath '$(cygpath -w "$work/unpacked")'"
  name=dist.exe
else
  tar -xJf "$archive" -C "$work/unpacked"
  name=dist
fi

binary="$(find "$work/unpacked" -type f -name "$name" | head -n 1)"
if [ -z "$binary" ]; then
  echo "::error::dist $version's archive for $target has no $name."
  exit 1
fi

bin="$HOME/.cargo/bin"
mkdir -p "$bin"
cp "$binary" "$bin/$name"
chmod +x "$bin/$name"
if [ "$ext" = zip ]; then
  cygpath -w "$bin" >> "$GITHUB_PATH"
else
  echo "$bin" >> "$GITHUB_PATH"
fi
"$bin/$name" --version
