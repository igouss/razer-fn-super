#!/usr/bin/env bash
set -euo pipefail
cd "${1:-$(dirname "$0")/..}"
found=0
while IFS= read -r -d '' file; do
    if grep -nE '^\s*//|/\*' "$file"; then
        echo "  in $file" >&2
        found=1
    fi
done < <(find . -path ./target -prune -o -name '*.rs' -print0)
while IFS= read -r -d '' file; do
    if grep -nE '^\s*#([^!\[]|$)' "$file"; then
        echo "  in $file" >&2
        found=1
    fi
done < <(find . -path ./target -prune -o \( -name '*.toml' -o -name '*.sh' -o -name '*.rules' \) -print0)
if [ "$found" -ne 0 ]; then
    echo "no-comments: code carries no comments; the why goes in docs/" >&2
    exit 1
fi
