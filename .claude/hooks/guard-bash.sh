#!/usr/bin/env bash
# PreToolUse guard for Bash: ask the user before privilege escalation or writes to a remote.
#
# Claude Code pipes the tool call as JSON on stdin. If the command contains sudo (or
# doas/pkexec/su), a git push in any form, or a GitHub CLI command that changes the
# remote, this prints a permissionDecision of "ask" so the user must approve it.
# Anything else passes through untouched. Matching errs on the side of asking.
# Needs only bash, grep and sed.

payload=$(cat)

# Pull out tool_input.command (a JSON string, escapes included), then undo the
# escapes that matter for matching: \n, \t and \".
command=$(printf '%s' "$payload" \
  | grep -o '"command"[[:space:]]*:[[:space:]]*"\([^"\\]\|\\.\)*"' \
  | head -n 1 \
  | sed -e 's/^"command"[[:space:]]*:[[:space:]]*"//' -e 's/"$//' \
        -e 's/\\n/\n/g' -e 's/\\t/\t/g' -e 's/\\"/"/g')

[ -z "$command" ] && exit 0

ask() {
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":"%s"}}\n' "$1"
  exit 0
}

# Characters that can come right before a command word: start of line, space,
# a separator, a subshell or backtick, or a path ("/usr/bin/git").
before='(^|[[:space:];&|(`$/])'

if printf '%s\n' "$command" | grep -Eq "${before}(sudo|doas|pkexec|su)([[:space:]]|$)"; then
  ask "Runs a command as root (sudo/doas/pkexec/su). Project rule: ask first."
fi

# "git", anything up to a separator, then "push" as a word: covers git push,
# git -C dir push, git --git-dir=x push, cd x && git push, \$(git push).
# `git stash push` only saves local work, so it is left alone.
if printf '%s\n' "$command" \
  | sed -E 's/stash[[:space:]]+push/stash save/g' \
  | grep -Eq "${before}git([[:space:]][^;&|\`]*)?[[:space:]]push([[:space:];&|)\`]|$)"; then
  ask "Pushes to a git remote. Project rule: ask before any push."
fi

gh_write='gh[[:space:]]+(pr[[:space:]]+(create|merge|close|reopen|edit|comment|review|ready)|release[[:space:]]+(create|delete|edit|upload)|repo[[:space:]]+(create|delete|edit|rename|sync|fork|archive)|workflow[[:space:]]+run|run[[:space:]]+(rerun|cancel)|api)([[:space:]]|$)'
if printf '%s\n' "$command" | grep -Eq "${before}${gh_write}"; then
  ask "Changes something on GitHub via the gh CLI. Project rule: ask before writing to the remote."
fi

exit 0
