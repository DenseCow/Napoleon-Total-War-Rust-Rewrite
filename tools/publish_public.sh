#!/usr/bin/env bash
# Publishes main's current tree to the public repo as one new snapshot commit on top of its main.
# The public repo shares no history with the private one (old commits hold personal paths), so this
# never pushes main itself. There is deliberately no `public` remote: a stray `git push public` from a
# worker would publish a branch. Usage: tools/publish_public.sh ["message"]
set -euo pipefail

URL="https://github.com/Rosebuddyy/Napoleon-Total-War-Rust-Rewrite.git"
MSG="${1:-Snapshot of the private main}"
cd "$(git rev-parse --show-toplevel)"

git fetch -q origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] \
  || { echo "HEAD is not origin/main: publish only what is merged and pushed" >&2; exit 1; }

# Personal-info scan of the tree being published (CLAUDE.md Hard rules).
user="${USERNAME:-$(basename "${USERPROFILE:-$HOME}")}"
if git grep -n -I -i -F "$user" HEAD -- . | head -5 | grep .; then
  echo "Windows username found in the tree; not publishing" >&2; exit 1
fi
if git grep -n -I -i -E '[A-Za-z0-9._%+-]+@(gmail|outlook|hotmail|yahoo|icloud|live|proton)\.' HEAD -- . | head -5 | grep .; then
  echo "Personal email found in the tree; not publishing" >&2; exit 1
fi
email="$(git config user.email)"
case "$email" in
  *@users.noreply.github.com) ;;
  *) echo "git user.email is $email, not a GitHub noreply address; not publishing" >&2; exit 1 ;;
esac

parent="$(git ls-remote "$URL" refs/heads/main | cut -f1)"
[ -n "$parent" ] || { echo "cannot read the public main" >&2; exit 1; }
git fetch -q "$URL" main

# The user's own session setup stays private: the launcher, day/night usage budget, its hooks and
# status line, the sandbox watchdog and the sandbox agent briefs. Testing and Ghidra tools are kept.
PRIVATE_ONLY=(
  start-claude.bat tools/start-claude.ps1 tools/usage_budget.ps1 tools/usage_sim.ps1
  tools/sandbox_watchdog.ps1 tools/guard_main_push.sh .claude/settings.json .opencode
)
index="$(mktemp)"
trap 'rm -f "$index"' EXIT
GIT_INDEX_FILE="$index" git read-tree HEAD
GIT_INDEX_FILE="$index" git rm -r -q --cached --ignore-unmatch -- "${PRIVATE_ONLY[@]}"
tree="$(GIT_INDEX_FILE="$index" git write-tree)"
if [ "$(git rev-parse "$parent^{tree}")" = "$tree" ]; then echo "public repo is already up to date"; exit 0; fi
commit="$(git commit-tree "$tree" -p "$parent" -m "$MSG (private $(git rev-parse --short HEAD))")"
git push -q "$URL" "$commit:refs/heads/main"
echo "published $commit"
