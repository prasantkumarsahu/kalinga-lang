#!/usr/bin/env bash
# Builds kalinga-lang.org into _site/ (or the folder given). Needs `kalinga` and `utkal` on
# PATH. The GitHub Pages workflow (.github/workflows/site.yml) runs this too.
#
#   site/build.sh            # then serve _site/ with any static file server
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/_site}"
case "$out" in /*) ;; *) out="$PWD/$out" ;; esac
rm -rf "$out"

# The pages: the book, the changelog, install and examples, search.txt, sitemap.xml.
(cd "$root/site" && utkal run .. "$out")

# The page script, written in Kalinga for the web: the search box and the Copy buttons.
(cd "$root/site/script" && utkal build >/dev/null && cp dist/app.js "$out/assets/site.js")

# Each live example runs on its own page, shown in an <iframe> on /play/.
for file in "$root"/examples/web/*.kl; do
  name="$(basename "$file" .kl)"
  kalinga build --web "$file" -o "$out/play/$name" >/dev/null
done

echo "site ready in $out"
