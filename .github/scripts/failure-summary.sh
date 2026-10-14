#!/usr/bin/env bash
# Makes a failed job's log readable without signing in to GitHub (raw job logs
# need a sign-in; annotations and the job summary do not): the job summary gets
# the panics and the end of the log, annotations get the panics, the error
# lines and the last 40 lines.
log="$1"
[ -f "$log" ] || { echo "no log at $log"; exit 0; }
clean() { sed 's/\x1b\[[0-9;]*m//g' "$@"; }
# Annotation text: % and newlines escaped, as workflow commands require.
escape() { sed 's/%/%25/g; s/\r/%0D/g' | awk 'BEGIN{ORS="%0A"} {print}'; }
{
  echo "### Failure log"
  echo
  echo '```text'
  clean "$log" | grep -A4 "panicked at" | head -n 120
  echo '```'
  echo
  echo '```text'
  clean "$log" | tail -n 200
  echo '```'
} >> "$GITHUB_STEP_SUMMARY"
# At most 10 error annotations per step are shown: panics (with the two lines
# after them) first, then other error lines.
{
  clean "$log" | grep -A2 "panicked at" | awk 'BEGIN{RS="--\n"} {gsub(/%/, "%25"); gsub(/\n/, "%0A"); print}'
  clean "$log" | grep -E "^(error(\[|:)|xtask: )|FAILED|failed \(" | sed 's/%/%25/g'
} | head -n 9 | while IFS= read -r line; do echo "::error title=log::${line}"; done
echo "::error title=log tail::$(clean "$log" | tail -n 40 | escape)"
exit 0
