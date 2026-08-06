#!/bin/sh
# Print one markdown section: the heading line through the next heading of
# equal or shallower depth (or EOF). Lets an agent read a slice of a file
# instead of the whole thing.
#
# Usage: sh .claude/scripts/extract-section.sh '### Heading text' path/to/file.md
set -eu

want="$1"
file="$2"

awk -v want="$want" '
/^#+[ \t]/ {
  n = match($0, /^#+/)
  lvl = RLENGTH
  if (printing && lvl <= start_lvl) { printing = 0 }
  if ($0 == want) { printing = 1; start_lvl = lvl; found = 1; print; next }
}
printing { print }
END { if (!found) { print "no section matched: " want > "/dev/stderr"; exit 1 } }
' "$file"
