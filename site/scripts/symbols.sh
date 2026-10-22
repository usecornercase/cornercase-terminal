#!/bin/sh
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
version=v3.5.1
sha256=01172f37db8543edb102e5cb5c64101c9f4686630804d49b419aa07b23a69996
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -sSfL -o "$tmp/symbols.tar.xz" "https://github.com/ryanoasis/nerd-fonts/releases/download/$version/NerdFontsSymbolsOnly.tar.xz"
if command -v sha256sum >/dev/null; then
  echo "$sha256  $tmp/symbols.tar.xz" | sha256sum -c -
else
  echo "$sha256  $tmp/symbols.tar.xz" | shasum -a 256 -c -
fi
mkdir "$tmp/font"
tar -xf "$tmp/symbols.tar.xz" -C "$tmp/font"
python3 -m venv "$tmp/venv"
"$tmp/venv/bin/pip" install -q fonttools==4.55.3 brotli==1.1.0
"$tmp/venv/bin/pyftsubset" "$tmp/font/SymbolsNerdFontMono-Regular.ttf" \
  --unicodes-file="$root/tests/fixtures/icons/symbols.txt" \
  --flavor=woff2 --layout-features='' --no-hinting --name-IDs='*' \
  --output-file="$root/src/web/assets/symbols.woff2"
cp "$root/src/web/assets/symbols.woff2" "$root/site/src/assets/symbols.woff2"
cp "$root/src/web/assets/symbols-LICENSE.txt" "$root/site/src/assets/symbols-LICENSE.txt"
