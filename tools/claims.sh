#!/usr/bin/env bash
# Claims on the public repo, so contributors and the maintainer's workers never take the same work.
# docs/BACKLOG.md on the maintainer's main is the one source of truth for the maintainer's side.
#
#   tools/claims.sh list        print the open claim issues (session start)
#   tools/claims.sh sync        rebuild the ONE pinned maintainer issue (label maintainer-claim) from
#                               docs/BACKLOG.md at origin/main (NR_BACKLOG_REF overrides the ref)
#   tools/claims.sh check N     (GitHub workflow) label claim issue N with its section and, when it
#                               overlaps the maintainer issue or another open claim, comment once
#                               and add needs-maintainer
#   tools/claims.sh mark-linked (GitHub workflow) label the open claims that have an open pull
#                               request closing them `linked-pr`, so they don't go stale
#
# A claim is a whole section (`## N.` or Polish), a subsection (a `### ` header, named by its first
# word, e.g. 0-B) or one item. The maintainer holds a section or subsection whose header is followed
# by a "**Reserved by the maintainer**" line, and every header or item line carrying
# `(running: <worker>)`. `clash` below is the one overlap rule for both sides.
#
# Every gh call has a timeout; a failure prints one line and exits non-zero (no retries).
set -uo pipefail

REPO="${NR_PUBLIC_REPO:-DenseCow/Napoleon-Total-War-Rust-Rewrite}"
TMO="${NR_GH_TIMEOUT:-15}"
MARKER='<!-- claims-bot -->'
TITLE='Reserved by the maintainer (updated automatically)'
CLAIMS_URL="https://github.com/$REPO/issues?q=is%3Aissue+is%3Aopen+label%3Aclaim"

ERR="$(mktemp)"
trap 'rm -f "$ERR"' EXIT

die() { echo "claims.sh: $*" >&2; exit 1; }

# Rows are tab-separated; bash `read` merges adjacent tabs (whitespace), losing empty fields, so
# loops read them with the unit separator instead.
US=$'\037'
us() { printf '%s\n' "$1" | tr '\t' "$US"; }

# gh with a timeout; on failure one line naming the call and gh's first error line.
ghc() {
  local out rc
  out="$(timeout "$TMO" gh "$@" 2>"$ERR")"; rc=$?
  if [ $rc -ne 0 ]; then
    [ $rc -eq 124 ] && die "gh $1 $2 timed out after ${TMO}s"
    die "gh $1 $2 failed: $(head -n 1 "$ERR")"
  fi
  printf '%s' "$out"
}

# Lower case, markdown and checkbox removed, spaces collapsed: how items and subsections compare.
norm() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | sed -E \
    -e 's/`//g; s/\*\*//g; s/\(running: [^)]*\)//g' \
    -e 's/^[[:space:]]*-[[:space:]]+//; s/^\[[ x]\][[:space:]]*//; s/^partly done:[[:space:]]*//' \
    -e 's/[[:space:]]+/ /g; s/^ //; s/ $//; s/\.$//'
}
# A subsection is named by its first word: "0-B campaign rules" -> "0-b".
norm_sub() { local s; s="$(norm "$1")"; s="${s#§}"; printf '%s' "${s%% *}"; }

# The overlap rule. Arguments: section, subsection, item of claim A, then the same for claim B,
# each already normalized (empty = the whole section / subsection).
clash() {
  [ "$1" = "$4" ] || return 1
  [ -z "$2$3" ] && return 0                                   # A is the whole section
  [ -z "$5$6" ] && return 0                                   # B is the whole section
  if [ -n "$2" ] && [ "$2" = "$5" ] && { [ -z "$3" ] || [ -z "$6" ]; }; then return 0; fi
  [ -n "$3" ] && [ "$3" = "$6" ]                              # the same item
}

# The maintainer's reservations from BACKLOG.md, one per line:
# section<TAB>subsection<TAB>item<TAB>holder<TAB>section title<TAB>subsection title
backlog_marks() {
  git show "${NR_BACKLOG_REF:-origin/main}:docs/BACKLOG.md" 2>"$ERR" \
    || die "cannot read docs/BACKLOG.md at ${NR_BACKLOG_REF:-origin/main}: $(head -n 1 "$ERR")"
}
parse_marks() {
  awk '
    function running(s) { return match(s, /\(running: [^)]+\)/) ? substr(s, RSTART + 10, RLENGTH - 11) : "" }
    function clean(s) {
      gsub(/\(running: [^)]+\)/, "", s); sub(/ — \[.*$/, "", s)
      sub(/^[ \t]*-[ \t]+/, "", s); sub(/^\[[ x]\][ \t]*/, "", s); sub(/^\*\*PARTLY DONE:\*\*[ \t]*/, "", s)
      gsub(/\t/, " ", s); gsub(/  +/, " ", s); sub(/^ /, "", s); sub(/ $/, "", s)
      return s
    }
    function out(sub_, item, who) { print sec "\t" sub_ "\t" item "\t" who "\t" stitle "\t" subtitle }
    { sub(/\r$/, "") }
    /^## / {
      sec = ""; sub_ = ""; subtitle = ""; pend = ""
      if (match($0, /^## [0-9]+\. /)) { sec = substr($0, 4, RLENGTH - 5); stitle = clean(substr($0, RLENGTH + 1)) }
      else if ($0 ~ /^## Polish/) { sec = "polish"; stitle = "Polish" }
      if (sec != "") { pend = "s"; w = running($0); if (w != "") out("", "", w) }
      next
    }
    /^### / && sec != "" {
      subtitle = clean(substr($0, 5)); sub_ = subtitle; sub(/ .*/, "", sub_); pend = "u"
      w = running($0); if (w != "") out(sub_, "", w)
      next
    }
    sec == "" || /^[ \t]*$/ { next }
    {
      if (pend != "" && index($0, "**Reserved by the maintainer**") == 1) out(pend == "u" ? sub_ : "", "", "maintainer")
      pend = ""
      if ($0 ~ /^[ \t]*- / && (w = running($0)) != "") out(sub_, clean($0), w)
    }'
}

# Human-readable line for one mark: section, subsection, item.
describe() {
  local sec="$1" sub="$2" item="$3" stitle="$4" subtitle="$5" d
  if [ "$sec" = polish ]; then d="Polish"; else d="§$sec $stitle"; fi
  [ -n "$sub" ] && d="$d › $subtitle"
  if [ -n "$item" ]; then d="$d: $item"; elif [ -n "$sub" ]; then d="$d (whole subsection)"; else d="$d (whole section)"; fi
  printf '%s' "$d"
}

ensure_labels() {
  local have want
  have="$(ghc label list -R "$REPO" --limit 500 --json name --jq '.[].name')" || exit 1
  for want in "$@"; do
    printf '%s\n' "$have" | grep -qxF -- "$want" && continue
    ghc label create "$want" -R "$REPO" --color "$(label_color "$want")" --description "$(label_desc "$want")" >/dev/null || exit 1
  done
}
label_color() { case "$1" in maintainer-claim) echo 5319E7 ;; stale) echo CCCCCC ;; needs-maintainer) echo D93F0B ;;
                linked-pr) echo 0E8A16 ;; claim) echo 0E8A16 ;; *) echo C5DEF5 ;; esac; }
label_desc() { case "$1" in
  maintainer-claim) echo "The maintainer's reserved BACKLOG work (tools/claims.sh sync)" ;;
  stale) echo "Claim with no activity; closes soon" ;;
  needs-maintainer) echo "Claim overlaps taken work; the maintainer will reply" ;;
  linked-pr) echo "Claim with an open pull request; never goes stale" ;;
  claim) echo "Someone is working on this BACKLOG item" ;;
  section:*) echo "BACKLOG §${1#section:}" ;; *) echo "" ;; esac; }

# The maintainer issue's number (lowest open or closed issue with the label), or nothing. REST, not
# `gh issue list`: its label filter lags a label change by minutes, so a sync right after labelling
# created a second issue (2026-10-08).
maintainer_issue() {
  ghc api "repos/$REPO/issues?labels=maintainer-claim&state=all&per_page=100" \
    --jq 'map(select(.pull_request | not) | .number) | min // empty'
}

cmd_list() {
  ghc issue list -R "$REPO" --label claim --state open --limit 200 || exit 1
  echo
}

cmd_sync() {
  local marks body human="" data="" n meta state pinned labels old_title old_body
  marks="$(backlog_marks | parse_marks)" || exit 1
  while IFS=$US read -r sec sub item who stitle subtitle; do
    [ -n "$sec" ] || continue
    if [ "$who" = maintainer ]; then human+="- $(describe "$sec" "$sub" "$item" "$stitle" "$subtitle")"$'\n'
    else human+="- $(describe "$sec" "$sub" "$item" "$stitle" "$subtitle") — worker \`$who\`"$'\n'; fi
    data+="$sec"$'\t'"$sub"$'\t'"${item//-->/-- >}"$'\n'
  done < <(us "$marks")
  [ -n "$human" ] || human="Nothing right now: no BACKLOG section, subsection or item is reserved or held by a maintainer worker."$'\n'
  body="$(cat <<EOF
The maintainer's own sessions hold the BACKLOG work below: please don't claim it or open pull requests for it while it is listed here. This issue is rebuilt from \`docs/BACKLOG.md\` on the maintainer's main by \`tools/claims.sh sync\` (edits here are overwritten); it stays open and pinned.

${human}
Everything else is open. Check the open [claim issues]($CLAIMS_URL), then open a "Claim a BACKLOG item" issue (CONTRIBUTING.md §3). A claim that overlaps this list or another open claim gets one bot comment and the \`needs-maintainer\` label.

<!-- claims-data (read by tools/claims.sh check: section, subsection, item)
${data}-->
EOF
)"
  ensure_labels claim maintainer-claim stale needs-maintainer linked-pr
  n="$(maintainer_issue)" || exit 1
  if [ -z "$n" ]; then
    n="$(printf '%s' "$body" | ghc issue create -R "$REPO" --title "$TITLE" --body-file - --label claim --label maintainer-claim)" || exit 1
    n="${n##*/}"
    ghc issue pin "$n" -R "$REPO" >/dev/null || exit 1
    echo "claims: created and pinned maintainer issue #$n"; return
  fi
  meta="$(ghc issue view "$n" -R "$REPO" --json state,isPinned,labels,title,body \
    --jq '.state + "\t" + (.isPinned|tostring) + "\t" + ([.labels[].name]|join(",")) + "\t" + .title + "\n" + .body')" || exit 1
  IFS=$US read -r state pinned labels old_title < <(us "${meta%%$'\n'*}")
  old_body="${meta#*$'\n'}"; old_body="${old_body//$'\r'/}"
  [ "$state" = OPEN ] || { ghc issue reopen "$n" -R "$REPO" >/dev/null || exit 1; }
  case ",$labels," in *,claim,*) ;; *) ghc issue edit "$n" -R "$REPO" --add-label claim >/dev/null || exit 1 ;; esac
  [ "$pinned" = true ] || { ghc issue pin "$n" -R "$REPO" >/dev/null || exit 1; }
  if [ "$old_title" = "$TITLE" ] && [ "$(printf '%s' "$old_body")" = "$(printf '%s' "$body")" ]; then
    echo "claims: maintainer issue #$n is up to date"; return
  fi
  printf '%s' "$body" | ghc issue edit "$n" -R "$REPO" --title "$TITLE" --body-file - >/dev/null || exit 1
  echo "claims: maintainer issue #$n updated"
}

# jq (gh --jq) that reads a claim issue form: section number or "polish", subsection, item.
JQ_CLAIM='
def field($n): (.body // "") | gsub("\r"; "")
  | ([capture("(?m)^### " + $n + "[ \t]*\n\\s*(?<v>[^\n]*)")] | first // {}) | (.v // "")
  | if test("^(_No response_|###)") then "" else gsub("\t"; " ") end;
def section: field("Section") as $s
  | if ($s | test("^§[0-9]+")) then ($s | capture("^§(?<n>[0-9]+)").n)
    elif ($s | ascii_downcase | startswith("polish")) then "polish"
    elif (.title | test("§[0-9]+")) then (.title | capture("§(?<n>[0-9]+)").n)
    else "" end;
def claim: [(.number|tostring), section, field("Subsection"), field("Item")] | join("\t");
def maintainer: [.labels[].name] | index("maintainer-claim") != null;'

cmd_check() {
  local n="$1" meta state labels marked sec sub item want m mdata others reasons="" add="" rm="" l have
  [[ "$n" =~ ^[0-9]+$ ]] || die "check needs an issue number"
  meta="$(ghc issue view "$n" -R "$REPO" --json number,state,labels,title,body,comments --jq "$JQ_CLAIM"'
    ([.state, ([.labels[].name]|join(",")),
      ([.comments[] | select((.author.login == "github-actions" or .author.login == "github-actions[bot]") and (.body | contains("'"$MARKER"'")))] | length | tostring)]
     | join("\t")) + "\n" + claim')" || exit 1
  IFS=$US read -r state labels marked < <(us "${meta%%$'\n'*}")
  IFS=$US read -r _ sec sub item < <(us "${meta#*$'\n'}")
  case ",$labels," in *,maintainer-claim,*) echo "claims: #$n is the maintainer issue"; return ;; esac
  case ",$labels," in *,claim,*) ;; *) echo "claims: #$n is not a claim"; return ;; esac
  [ "$state" = OPEN ] || { echo "claims: #$n is closed"; return; }
  [ -n "$sec" ] || { echo "claims: #$n names no BACKLOG section; nothing to check"; return; }

  # The section label, so the issue list filters per section.
  want="section:$sec"
  case ",$labels," in *",$want,"*) ;; *) add="$want" ;; esac
  IFS=, read -r -a have <<<"$labels"
  for l in "${have[@]}"; do case "$l" in section:*) [ "$l" = "$want" ] || rm+="${rm:+,}$l" ;; esac; done
  if [ -n "$add$rm" ]; then
    [ -n "$add" ] && { ensure_labels "$add" || exit 1; }
    ghc issue edit "$n" -R "$REPO" ${add:+--add-label "$add"} ${rm:+--remove-label "$rm"} >/dev/null || exit 1
  fi

  [ "$marked" = 0 ] || { echo "claims: #$n already has the overlap comment"; return; }
  sec="$(norm "$sec")"; sub="$(norm_sub "$sub")"; item="$(norm "$item")"

  m="$(maintainer_issue)" || exit 1
  if [ -n "$m" ]; then
    mdata="$(ghc issue view "$m" -R "$REPO" --json body \
      --jq '(.body // "") | gsub("\r"; "") | [capture("(?s)<!-- claims-data[^\n]*\n(?<d>.*?)-->")] | (first // {}) | (.d // "")')" || exit 1
    while IFS=$US read -r s u i; do
      [ -n "$s" ] || continue
      clash "$sec" "$sub" "$item" "$(norm "$s")" "$(norm_sub "$u")" "$(norm "$i")" \
        && reasons+="- The maintainer holds §$s${u:+ $u}${i:+: $i} (see #$m)."$'\n'
    done < <(us "$mdata")
  fi
  others="$(ghc issue list -R "$REPO" --label claim --state open --limit 500 --json number,title,body,labels \
    --jq "$JQ_CLAIM"' .[] | select(maintainer | not) | claim')" || exit 1
  while IFS=$US read -r k s u i; do
    [ -n "$k" ] && [ "$k" != "$n" ] || continue
    clash "$sec" "$sub" "$item" "$(norm "$s")" "$(norm_sub "$u")" "$(norm "$i")" \
      && reasons+="- Open claim #$k covers an overlapping part of the same section."$'\n'
  done < <(us "$others")

  [ -n "$reasons" ] || { echo "claims: #$n overlaps nothing"; return; }
  ensure_labels needs-maintainer
  printf '%s\n' "$MARKER" "This claim overlaps work that is already taken:" "" "$reasons" \
    "Please wait for the maintainer before starting; see CONTRIBUTING.md §3. Edit the claim to pick something else, or comment \`/unclaim\` to release it." \
    | ghc issue comment "$n" -R "$REPO" --body-file - >/dev/null || exit 1
  ghc issue edit "$n" -R "$REPO" --add-label needs-maintainer >/dev/null || exit 1
  echo "claims: #$n overlaps taken work; commented"
}

cmd_mark_linked() {
  local owner="${REPO%%/*}" name="${REPO#*/}" rows k has labelled
  ensure_labels linked-pr
  rows="$(ghc api graphql -f owner="$owner" -f name="$name" -f query='
    query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {
      issues(states: OPEN, labels: ["claim"], first: 100) { nodes { number
        labels(first: 50) { nodes { name } }
        closedByPullRequestsReferences(first: 10, includeClosedPrs: false) { totalCount } } } } }' \
    --jq '.data.repository.issues.nodes[] | [.number, (.closedByPullRequestsReferences.totalCount > 0),
          ([.labels.nodes[].name] | index("linked-pr") != null)] | map(tostring) | join("\t")')" || exit 1
  while IFS=$US read -r k has labelled; do
    [ -n "$k" ] || continue
    if [ "$has" = true ] && [ "$labelled" = false ]; then
      ghc issue edit "$k" -R "$REPO" --add-label linked-pr >/dev/null || exit 1; echo "claims: #$k has an open pull request"
    elif [ "$has" = false ] && [ "$labelled" = true ]; then
      ghc issue edit "$k" -R "$REPO" --remove-label linked-pr >/dev/null || exit 1; echo "claims: #$k has no open pull request now"
    fi
  done < <(us "$rows")
}

case "${1:-}" in
  list) cmd_list ;;
  sync) cmd_sync ;;
  check) cmd_check "${2:-}" ;;
  mark-linked) cmd_mark_linked ;;
  *) echo "usage: tools/claims.sh list | sync | check <issue> | mark-linked" >&2; exit 2 ;;
esac
