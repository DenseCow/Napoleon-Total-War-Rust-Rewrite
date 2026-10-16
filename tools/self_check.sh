#!/usr/bin/env bash
# Pre-report self-check for a worker's branch.
# Usage: bash tools/self_check.sh [base] [head]   (base defaults to main, head to HEAD)
# Run it inside the branch's worktree before reporting. It checks, on the diff only. The diff is
# merge-base(base, head) against head; when head is HEAD it is against the working tree, so uncommitted
# changes to tracked files count too (untracked files are listed in a note, not checked: `git add -N` them).
#  1. Ghidra V5: every exe address (0x + 6 to 8 hex digits, padded to 8) on an added line that is a function
#     entry with a non-FUN_ name needs a raw completeness score >= 70 (field "completeness_score" of the
#     plugin's /analyze_function_completeness, NOT "effective_score"). Needs Ghidra's HTTP plugin on
#     127.0.0.1:8089 (override: GHIDRA_URL) with Napoleon.exe open. The diff is netted first: a line
#     moved elsewhere counts as neither added nor removed. Rules, keyed on the Ghidra NAME:
#       - low score and the branch introduced the name (it is not known at the merge base): FAIL;
#       - low score and the name is known at the merge base: WARN list, no failure. Known = the name
#         occurs there on a line that also holds the address, or the address occurs there and the name
#         is not on the net added lines;
#       - "No function at address" is skipped; an unreachable Ghidra, timeout, empty reply, other plugin
#         error or missing score: FAIL for that address.
#  2. Stale references: identifiers (fn/const/static/struct/enum/trait/type/field names, 6+ chars) removed
#     from .rs files, not re-added, no definition of that name left, yet still found in tracked .rs,
#     docs/*.md or analysis/*.md. A `name:` field counts only when the removed line looks like a field
#     definition (a type after the colon), not a struct-literal field.
#  3. Tags: PROVISIONAL/PLACEHOLDER/INFERRED lines added/removed (info only; cite evidence for removals).
# Exit 0 only when checks 1 and 2 find nothing (WARN lines do not count).
set -u
BASE=${1:-main}; HEAD_REV=${2:-HEAD}
GHIDRA=${GHIDRA_URL:-http://127.0.0.1:8089}
fail=0
D=$(mktemp -d) || { echo "mktemp failed"; exit 2; }
trap 'rm -rf "$D"' EXIT

MB=$(git merge-base "$BASE" "$HEAD_REV") || { echo "no merge-base for $BASE and $HEAD_REV"; exit 2; }
if [ "$HEAD_REV" = HEAD ]; then
  git diff -U0 --no-color "$MB" -- ':/' ':(top,exclude)*.lock' >"$D/diff" || { echo "git diff failed"; exit 2; }
  un=$(git ls-files --others --exclude-standard | wc -l)
  [ "$un" -gt 0 ] && echo "NOTE: $un untracked file(s) are not checked (git add -N them to include)"
  [ -n "$(git status --porcelain --untracked-files=no)" ] && echo "NOTE: uncommitted changes to tracked files are included"
else
  git diff -U0 --no-color "$MB" "$HEAD_REV" -- ':/' ':(top,exclude)*.lock' >"$D/diff" || { echo "git diff failed"; exit 2; }
fi

# git grep over the head's tree (the working tree when head is HEAD, so uncommitted changes count).
# Args: options and patterns, then `--` and repo-root pathspecs. Prints path:line:text. Exit 1 = no match.
hgrep() {
  local pre=() 
  while [ $# -gt 0 ] && [ "$1" != -- ]; do pre+=("$1"); shift; done
  if [ "$HEAD_REV" = HEAD ]; then git grep -n --no-color "${pre[@]}" "$@"; return $?; fi
  git grep -n --no-color "${pre[@]}" "$HEAD_REV" "$@" >"$D/hg.out"; local g=$?
  awk -v p="$HEAD_REV:" 'index($0,p)==1{$0=substr($0,length(p)+1)}1' "$D/hg.out"
  return $g
}

# diff -> "A|file|line|text" / "R|file|line|text"; --- and +++ are headers only before the first @@
awk '
/^diff --git/{inh=0; next}
inh==0 && /^--- /{o=substr($0,5); sub(/^a\//,"",o); next}
inh==0 && /^\+\+\+ /{f=substr($0,5); sub(/^b\//,"",f); next}
/^@@/{ inh=1; match($0,/-[0-9]+/); ol=substr($0,RSTART+1,RLENGTH-1)+0; match($0,/\+[0-9]+/); nl=substr($0,RSTART+1,RLENGTH-1)+0; next}
inh==1 && /^\+/{sub(/\015$/,""); print "A|" f "|" nl "|" substr($0,2); nl++; next}
inh==1 && /^-/{sub(/\015$/,""); print "R|" o "|" ol "|" substr($0,2); ol++; next}
' "$D/diff" >"$D/lines"

# A pure code move must not flag anything: compare added and removed lines as a multiset (leading
# whitespace ignored), so a line removed in one place and added in another counts as neither. Only the
# net added and net removed lines stay in "lines": the first min(added, removed) copies of a text drop.
awk '
{ t=$0; sub(/^[^|]*\|[^|]*\|[^|]*\|/,"",t); sub(/^[ \t]+/,"",t)
  line[NR]=$0; kind[NR]=substr($0,1,1); txt[NR]=t
  if (kind[NR]=="A") a[t]++; else r[t]++ }
END{ for(i=1;i<=NR;i++){ t=txt[i]; m=(a[t]<r[t])?a[t]:r[t]
  if (kind[i]=="A") { if (++sa[t]<=m) continue } else { if (++sr[t]<=m) continue }
  print line[i] } }
' "$D/lines" >"$D/lines.net" && mv "$D/lines.net" "$D/lines"

# addresses of a diff side, normalised to 8 upper-case hex digits
addrs_of() {
  grep "^$1|" "$D/lines" | cut -d'|' -f4- | grep -oiE '\b0x[0-9a-f]{6,8}\b' \
    | while read -r x; do printf '%08X\n' "0x${x#0[xX]}"; done | sort -u
}

grep '^A|' "$D/lines" | cut -d'|' -f4- >"$D/addedtext"

echo "== 1. Ghidra V5 completeness (raw completeness_score < 70) =="
addrs_of A >"$D/addrs"
n=$(wc -l <"$D/addrs")
if [ "$n" -eq 0 ]; then echo "no exe addresses added"
elif ! curl.exe -s -m 10 "$GHIDRA/check_connection" | grep -q '"ok"'; then
  echo "FAIL: Ghidra not reachable at $GHIDRA (start it with Napoleon.exe open); $n addresses unchecked"; fail=1
else
  checked=0; lownew=0; lowold=0; errs=0
  : >"$D/warn"
  while read -r a; do
    r=$(curl.exe -s -m 60 "$GHIDRA/analyze_function_completeness?function_address=0x$a&program=/Napoleon.exe"); rc=$?
    if [ $rc -ne 0 ]; then echo "FAIL  0x$a  curl error $rc (timeout or connection lost)"; errs=$((errs+1)); fail=1; continue; fi
    if [ -z "$r" ]; then echo "FAIL  0x$a  empty reply"; errs=$((errs+1)); fail=1; continue; fi
    case "$r" in
      *'"error":"No function at'*) continue;;                      # not a function: a constant or data address
      *'"error"'*) echo "FAIL  0x$a  plugin error: $(echo "$r" | head -c 160)"; errs=$((errs+1)); fail=1; continue;;
      *'"function_name"'*) ;;
      *) echo "FAIL  0x$a  unexpected reply: $(echo "$r" | head -c 160)"; errs=$((errs+1)); fail=1; continue;;
    esac
    name=$(echo "$r" | grep -o '"function_name":"[^"]*"' | head -1 | cut -d'"' -f4)
    if [ -z "$name" ]; then echo "FAIL  0x$a  reply has no function name"; errs=$((errs+1)); fail=1; continue; fi
    case "$name" in FUN_*) continue;; esac
    score=$(echo "$r" | grep -o '"completeness_score":[-0-9.eE]*' | head -1 | cut -d: -f2)
    if [ -z "$score" ]; then echo "FAIL  0x$a  $name  no completeness_score in reply"; errs=$((errs+1)); fail=1; continue; fi
    checked=$((checked+1))
    if awk "BEGIN{exit !($score < 70)}"; then
      # known at the merge base = the name occurs there on a line that also holds this address (a bare name
      # mention, e.g. a planned name or a short word, does not count); or the address occurs at the base and
      # the name is not on the net added lines (a thunk named after its callee, cited without its name).
      # Anything else the branch introduced.
      a0=$(echo "$a" | sed 's/^0*//')
      git grep -n -w -F -e "$name" "$MB" -- ':/' ':(top,exclude)*.lock' | grep -iE "0x0*$a0([^0-9a-f]|$)" >/dev/null; ps=("${PIPESTATUS[@]}"); g=${ps[1]}
      [ "${ps[0]}" -gt 1 ] && g=2
      if [ $g -eq 1 ] && ! grep -qw -F -e "$name" "$D/addedtext" && git grep -q -w -F -e "$name" "$MB" -- ':/' ':(top,exclude)*.lock'; then
        git grep -n -i -E -e "0x0*$a0([^0-9a-f]|$)" "$MB" -- ':/' ':(top,exclude)*.lock' >/dev/null; g2=$?
        [ $g2 -le 1 ] && g=$g2 || g=2
      fi
      if [ $g -eq 0 ]; then
        printf '0x%s  %s  score %.1f\n' "$a" "$name" "$score" >>"$D/warn"; lowold=$((lowold+1))
      elif [ $g -eq 1 ]; then
        printf 'LOW  0x%s  %s  score %.1f  (name new to the repo)\n' "$a" "$name" "$score"; lownew=$((lownew+1)); fail=1
      else
        echo "FAIL  0x$a  git grep failed at base"; errs=$((errs+1)); fail=1
      fi
    fi
  done <"$D/addrs"
  echo "$n addresses added, $checked named function entries checked, $lownew new below 70, $lowold known at the base, $errs errors"
  if [ -s "$D/warn" ]; then echo "WARN known at the merge base (below 70, not failing):"; sed 's/^/    /' "$D/warn"; fi
fi

echo "== 2. Removed identifiers still referenced =="
# a field type: a path/generic type, a slice/array or a tuple (never a call, so not a struct-literal value)
P="\(([^()]|\([^()]*\))*\)"   # a paren group with one nested level
TYPE="(&?'?(dyn[[:space:]]+|impl[[:space:]]+)?[A-Za-z_]([A-Za-z0-9_:<>, ;&'+]|->|\[|\]|(Fn|FnMut|FnOnce)$P)*|(unsafe[[:space:]]+)?(extern[[:space:]]+\"[^\"]*\"[[:space:]]+)?fn$P[^,;()]*|\*(const|mut)[[:space:]]+[A-Za-z0-9_:<>*]+|\[[^()]*\]|$P)"
FDEF="[[:space:]]*,?[[:space:]]*(//.*)?\$"
grep '^R|[^|]*\.rs|' "$D/lines" | cut -d'|' -f4- >"$D/removedtext"
{
  # items; leftmost-longest also takes `const fn name`
  grep -oE '(fn|const|static|struct|enum|trait|type)[[:space:]]+(mut[[:space:]]+|fn[[:space:]]+)?[A-Za-z_][A-Za-z0-9_]*' "$D/removedtext" \
    | sed -E 's/^(fn|const|static|struct|enum|trait|type)[[:space:]]+(mut[[:space:]]+|fn[[:space:]]+)?//; s/^/I /'
  # fields: only lines that look like a definition (`name: Type,`), not struct-literal / pattern fields
  sed -nE "s#^[[:space:]]*(pub(\\([a-z]+\\))?[[:space:]]+)?([a-z_][a-z0-9_]*):[[:space:]]*($TYPE)$FDEF#\\3 \\4#p" "$D/removedtext" \
    | awk '{v=$2; if (v ~ /^(true|false)$/) next; if (NF>=3 && $3 != "->" && $3 ~ /^[-+*\/%|^&<>=]+$/ && v !~ /^(dyn|impl)$/) next; if (v ~ /^[a-z_][a-z0-9_]*[,;]?$/ && v !~ /^(extern|unsafe|impl|dyn)[,;]?$/ && v !~ /^(u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize|f32|f64|bool|str|char)[,;]?$/) next; print "F " $1}'
} | awk 'length($2)>=6' | sort -u >"$D/removed"
stale=0
while read -r kind id; do
  [ -z "$id" ] && continue
  grep -qw -- "$id" "$D/addedtext" && continue
  # only report distinctive names (underscore, or CamelCase/CONST) to avoid common words
  echo "$id" | grep -qE '_|[a-z][A-Z]|^[A-Z][a-z]+[A-Z]|^[A-Z0-9]+$' || continue
  # a definition of the same name still at head (another impl or struct) means it was not removed
  if [ "$kind" = F ]; then dp="^[[:space:]]*(pub(\\([a-z]+\\))?[[:space:]]+)?$id:[[:space:]]*$TYPE$FDEF"
  else dp="(fn|const|static|struct|enum|trait|type)[[:space:]]+(mut[[:space:]]+|fn[[:space:]]+)?$id\\b"; fi
  hgrep -E -e "$dp" -- ':/*.rs' >"$D/defs"; g=$?
  if [ $g -gt 1 ]; then echo "FAIL: git grep failed for '$id'"; fail=1; continue; fi
  [ $g -eq 0 ] && continue
  hgrep -w -E -e "$id" -- ':/*.rs' ':/docs/*.md' ':/analysis/*.md' >"$D/hits.all"; g=$?
  if [ $g -gt 1 ]; then echo "FAIL: git grep failed for '$id'"; fail=1; continue; fi
  if [ $g -eq 0 ]; then
    echo "STALE '$id':"; head -5 "$D/hits.all" | sed 's/^/    /'; stale=$((stale+1)); fail=1
  fi
done <"$D/removed"
echo "$(wc -l <"$D/removed") removed identifiers considered, $stale still referenced"

echo "== 3. Tags (info) =="
for k in A R; do
  [ $k = A ] && w=added || w=removed
  c=$(grep "^$k|" "$D/lines" | grep -cE 'PROVISIONAL|PLACEHOLDER|INFERRED')
  echo "-- $w: $c"
  grep "^$k|" "$D/lines" | grep -E 'PROVISIONAL|PLACEHOLDER|INFERRED' \
    | sed -E 's/^[AR]\|([^|]*)\|([0-9]*)\|[[:space:]]*(.*)$/   \1:\2: \3/'
done
echo "(each removed tag needs cited evidence in the report)"

if [ $fail -eq 0 ]; then echo "SELF-CHECK PASSED"; else echo "SELF-CHECK FAILED"; fi
exit $fail
