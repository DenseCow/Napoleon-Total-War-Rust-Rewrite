#!/usr/bin/env bash
# Rewrites the "## Progress" block of docs/BACKLOG.md from tools/backlog_count.sh and
# tools/tag_count.sh, so the table and the tag line are never edited by hand. The manager runs it
# in each merge commit (CLAUDE.md, token discipline). Earlier values are in git history.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

rows="$(bash tools/backlog_count.sh | grep -E '^\| [0-9]+\. ')"
[ -n "$rows" ] || { echo "progress.sh: backlog_count.sh printed no section rows" >&2; exit 1; }
read -r p ph inf unk < <(bash tools/tag_count.sh | awk '$1 == "total" { print $2, $3, $4, $5 }')
[ -n "${unk:-}" ] || { echo "progress.sh: tag_count.sh printed no total line" >&2; exit 1; }
total=$((p + ph + inf))
today="$(date +%Y-%m-%d)"

block="$(mktemp)"
trap 'rm -f "$block"' EXIT
{
  printf '## Progress\n\n'
  printf 'Generated %s by `bash tools/progress.sh` (from `tools/backlog_count.sh` and `tools/tag_count.sh`); never edit it by hand.\n\n' "$today"
  printf 'Tags left in the code (done means zero): %s (%s PROVISIONAL, %s PLACEHOLDER, %s INFERRED), plus %s UNKNOWN.\n\n' "$total" "$p" "$ph" "$inf" "$unk"
  printf '| Section | Done | Partly done | To do |\n|---|---|---|---|\n%s\n' "$rows"
} >"$block"

# Replace from "## Progress" up to (not including) the first line after the table that is not a
# table row; everything else in the file is kept byte for byte (line endings included).
awk -v blockfile="$block" '
  BEGIN { while ((getline l < blockfile) > 0) block = block l "\n" }
  /^## Progress\r?$/ { printf "%s", block; skip = 1; intable = 0; next }
  skip && /^\|/ { intable = 1; next }
  skip && intable && !/^\|/ { skip = 0 }
  skip { next }
  { print }
' docs/BACKLOG.md >"$block.out"
grep -q '^## Progress' "$block.out" || { echo "progress.sh: no ## Progress block found" >&2; rm -f "$block.out"; exit 1; }
mv "$block.out" docs/BACKLOG.md
echo "progress: $total tags + $unk UNKNOWN; $(printf '%s\n' "$rows" | wc -l) sections"
