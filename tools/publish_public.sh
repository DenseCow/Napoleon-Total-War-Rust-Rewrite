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
tree="$(git rev-parse 'HEAD^{tree}')"
if [ "$(git rev-parse "$parent^{tree}")" = "$tree" ]; then echo "public repo is already up to date"; exit 0; fi
commit="$(git commit-tree "$tree" -p "$parent" -m "$MSG (private $(git rev-parse --short HEAD))")"
git push -q "$URL" "$commit:refs/heads/main"
echo "published $commit"
