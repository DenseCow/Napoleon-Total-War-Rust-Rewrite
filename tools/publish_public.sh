#!/usr/bin/env bash
# The public repo is a mirror of the private main: each publish is one new snapshot commit on top of
# the public main. It shares no history with the private repo (old commits hold personal paths), so
# this never pushes main itself. There is deliberately no `public` remote: a stray `git push public`
# from a worker would publish a branch.
#
#   tools/publish_public.sh ["message"]   publish main's tree (manager, after a merge)
#   tools/publish_public.sh import        bring contributor commits merged on the public main into a
#                                         private branch (worktree NR-public-import) for review
#
# Contributor pull requests are merged on the public repo. A snapshot replaces the public tree with
# the private one, so it would silently undo those commits: publishing refuses until every one of
# them is in the private main (imported with `cherry-pick -x`, reviewed and merged).
set -euo pipefail

URL="${NR_PUBLIC_URL:-https://github.com/Rosebuddyy/Napoleon-Total-War-Rust-Rewrite.git}"  # override only for tests
SNAPSHOT_COMMITTER="206947766+Rosebuddyy@users.noreply.github.com"
cd "$(git rev-parse --show-toplevel)"

# The user's own session setup stays private: the launcher, day/night usage budget, its hooks and
# status line, the sandbox watchdog and the sandbox agent briefs. Testing and Ghidra tools are kept.
PRIVATE_ONLY=(
  start-claude.bat tools/start-claude.ps1 tools/usage_budget.ps1 tools/usage_sim.ps1
  tools/sandbox_watchdog.ps1 tools/guard_main_push.sh .claude/settings.json .opencode
)
# Paths that run code or steer agents on the maintainer's machine: an import that touches them
# needs a line-by-line review before it merges.
SENSITIVE='^(\.github/|\.claude/|\.mcp\.json|\.cargo/|CLAUDE\.md|AGENTS\.md|tools/|Cargo\.toml|Cargo\.lock|.*build\.rs$|.*\.(bat|cmd|ps1|sh)$)'

git fetch -q origin main
git fetch -q "$URL" main
public="$(git rev-parse FETCH_HEAD)"

# The newest snapshot on the public main, and the contributor commits after it that the private main
# does not have yet.
last="$(git log --first-parent -1 --format=%H --committer="<${SNAPSHOT_COMMITTER//+/\\+}>" -E \
  --grep='\(private [0-9a-f]{7,}\)$|\(public snapshot\)' "$public")"
[ -n "$last" ] || { echo "no snapshot found on the public main" >&2; exit 1; }
pending=()
for c in $(git rev-list --reverse --no-merges "$last..$public"); do
  [ -n "$(git log origin/main -1 --format=%H -F --grep="cherry picked from commit $c")" ] || pending+=("$c")
done

if [ "${1:-}" = "import" ]; then
  [ ${#pending[@]} -gt 0 ] || { echo "nothing to import: the private main has every public commit"; exit 0; }
  for c in "${pending[@]}"; do
    echo "$(git log -1 --format='%h %an: %s' "$c")"
    files="$(git diff-tree --no-commit-id --name-only -r "$c")"
    for p in "${PRIVATE_ONLY[@]}"; do
      if printf '%s\n' "$files" | grep -q -F -x -e "$p" || printf '%s\n' "$files" | grep -q "^$p/"; then
        echo "  touches private-only path $p: not importing; handle it by hand" >&2; exit 1
      fi
    done
    printf '%s\n' "$files" | grep -E "$SENSITIVE" | sed 's/^/  REVIEW CLOSELY (runs code or steers agents): /' || true
  done
  wt="$(dirname "$PWD")/NR-public-import"
  branch="public-import/$(git rev-parse --short "${pending[-1]}")"
  [ ! -e "$wt" ] || { echo "$wt exists: finish or remove the previous import first" >&2; exit 1; }
  git worktree add -q "$wt" -b "$branch" origin/main
  git -C "$wt" cherry-pick -x "${pending[@]}" \
    || { echo "conflict: resolve it in $wt, then git -C $wt cherry-pick --continue" >&2; exit 1; }
  echo "imported ${#pending[@]} commit(s) into $branch at $wt."
  echo "Review and test it like any branch, merge it into main, push, then publish."
  exit 0
fi

MSG="${1:-Snapshot of the private main}"
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] \
  || { echo "HEAD is not origin/main: publish only what is merged and pushed" >&2; exit 1; }
if [ ${#pending[@]} -gt 0 ]; then
  echo "the public main has ${#pending[@]} contributor commit(s) the private main lacks; publishing would undo them:" >&2
  for c in "${pending[@]}"; do git log -1 --format='  %h %an: %s' "$c" >&2; done
  echo "run: tools/publish_public.sh import" >&2; exit 1
fi

# Personal-info scan of the tree being published (CLAUDE.md Hard rules).
user="${USERNAME:-$(basename "${USERPROFILE:-$HOME}")}"
if git grep -n -I -i -F "$user" HEAD -- . | head -5 | grep .; then
  echo "Windows username found in the tree; not publishing" >&2; exit 1
fi
if git grep -n -I -i -E '[A-Za-z0-9._%+-]+@(gmail|outlook|hotmail|yahoo|icloud|live|proton)\.' HEAD -- . | head -5 | grep .; then
  echo "Personal email found in the tree; not publishing" >&2; exit 1
fi
[ "$(git config user.email)" = "$SNAPSHOT_COMMITTER" ] \
  || { echo "git user.email is not $SNAPSHOT_COMMITTER; not publishing" >&2; exit 1; }

index="$(mktemp)"
trap 'rm -f "$index"' EXIT
GIT_INDEX_FILE="$index" git read-tree HEAD
GIT_INDEX_FILE="$index" git rm -r -q --cached --ignore-unmatch -- "${PRIVATE_ONLY[@]}"
tree="$(GIT_INDEX_FILE="$index" git write-tree)"
if [ "$(git rev-parse "$public^{tree}")" = "$tree" ]; then echo "public repo is already up to date"; exit 0; fi
commit="$(git commit-tree "$tree" -p "$public" -m "$MSG (private $(git rev-parse --short HEAD))")"
git push -q "$URL" "$commit:refs/heads/main"
echo "published $commit"
