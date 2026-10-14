#!/usr/bin/env bash
# Puts the end of a failed job's log into the job summary and the most telling
# lines into annotations: both are readable without signing in to GitHub,
# unlike the raw job log.
log="$1"
[ -f "$log" ] || { echo "no log at $log"; exit 0; }
clean() { sed 's/\x1b\[[0-9;]*m//g' "$@"; }
{
  echo "### Failure log"
  echo
  echo "Panics with their messages:"
  echo
  echo '```text'
  clean "$log" | grep -A4 "panicked at" | head -n 120
  echo '```'
  echo
  echo "Last 200 lines:"
  echo
  echo '```text'
  clean "$log" | tail -n 200
  echo '```'
} >> "$GITHUB_STEP_SUMMARY"
# Annotations (at most 10 per step are shown): each panic with the two lines
# after it first, then the other error lines.
{
  clean "$log" | grep -A2 "panicked at" | awk 'BEGIN{RS="--\n"} {gsub(/\n/, "%0A"); print}'
  clean "$log" | grep -E "^(error(\[|:)|xtask: )|FAILED|failed \("
} | head -n 10 | while IFS= read -r line; do echo "::error title=log::${line}"; done
exit 0
