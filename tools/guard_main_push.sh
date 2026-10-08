#!/usr/bin/env bash
# PreToolUse guard (Bash and PowerShell tools): only the local manager pushes to main (CLAUDE.md,
# "Agent workflow"). Sessions not on the user's machine (cloud / claude.ai/code) are refused any
# `git push` that names main, pushes --all/--mirror, or runs while main is checked out.
# Exit 2 blocks the tool call and shows the reason to Claude.
# NR_GUARD_FORCE_REMOTE=1 treats this machine as remote (for testing the block; stricter only).

input=$(cat)
cmd=$(printf '%s' "$input" | tr -d '\r\n' | sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\(.*\)".*/\1/p')

# Not a git push: nothing to check.
printf '%s' "$cmd" | grep -Eq 'git([[:space:]]+-C[[:space:]]+[^[:space:]]+)?[[:space:]]+push' || exit 0

# The user's machine (it has the original's install): the local manager's own merges.
if [ "${NR_GUARD_FORCE_REMOTE:-0}" != "1" ] && [ -d "/c/Program Files (x86)/Steam/steamapps/common/Napoleon Total War" ]; then
    exit 0
fi

block() {
    echo "Blocked: only the local manager session pushes to main (CLAUDE.md, Agent workflow). $1 Commit to your own branch, push that branch, and ask for a merge." >&2
    exit 2
}

push_part=$(printf '%s' "$cmd" | sed -n 's/.*push\(.*\)/\1/p')
printf '%s' "$push_part" | grep -Eq '(^|[[:space:]:+/])main([[:space:];&|"\\]|$)' && block "The push names main."
printf '%s' "$push_part" | grep -Eq '(^|[[:space:]])--(all|mirror)([[:space:]]|$)' && block "--all/--mirror would push main."

cwd=$(printf '%s' "$input" | tr -d '\r\n' | sed -n 's/.*"cwd"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
branch=$(git -C "${cwd:-.}" rev-parse --abbrev-ref HEAD 2>/dev/null)
[ "$branch" = "main" ] && block "main is checked out, so this push would update main."
exit 0
