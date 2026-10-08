#!/usr/bin/env bash
# Recounts docs/BACKLOG.md per numbered section (done / partly done / to do), as the Progress table shows it.
# Usage: bash tools/backlog_count.sh   (from anywhere in the repo). Paste the rows into the Progress table.
cd "$(git rev-parse --show-toplevel)" || exit 1
awk '
/^## [0-9]+\./ { s = $2; sub(/\.$/, "", s); name[s] = $0; sub(/^## [0-9]+\. /, "", name[s]); order[++n] = s; next }
/^## / { s = ""; next }
s != "" && /^- \[x\]/ { d[s]++; next }
s != "" && /^- \[ \] \*\*PARTLY DONE/ { p[s]++; next }
s != "" && /^- \[ \]/ { t[s]++ }
END { for (i = 1; i <= n; i++) { k = order[i]; printf "| %s. %s | %d | %d | %d |\n", k, name[k], d[k], p[k], t[k] } }
' docs/BACKLOG.md
