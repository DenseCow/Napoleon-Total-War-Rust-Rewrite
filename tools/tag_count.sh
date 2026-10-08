#!/usr/bin/env bash
# Counts the fidelity tags left in the code (CLAUDE.md "Done means zero tags"), per crate and in total.
# Usage: bash tools/tag_count.sh   (from the repo root; reads tracked files only)
cd "$(git rev-parse --show-toplevel)" || exit 1
printf '%-14s %12s %12s %10s %8s\n' crate PROVISIONAL PLACEHOLDER INFERRED UNKNOWN
for dir in crates/*/; do
    crate=$(basename "$dir")
    counts=""
    for tag in PROVISIONAL PLACEHOLDER INFERRED UNKNOWN; do
        n=$(git grep -o -w "$tag" -- "$dir*.rs" "$dir**/*.rs" 2>/dev/null | wc -l)
        counts="$counts $n"
    done
    # shellcheck disable=SC2086
    printf '%-14s %12s %12s %10s %8s\n' "$crate" $counts
done
printf '%-14s' total
for tag in PROVISIONAL PLACEHOLDER INFERRED UNKNOWN; do
    n=$(git grep -o -w "$tag" -- 'crates/*.rs' 2>/dev/null | wc -l)
    printf ' %12s' "$n"
done
printf '\n'
