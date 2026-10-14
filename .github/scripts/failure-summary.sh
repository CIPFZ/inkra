#!/usr/bin/env bash
# Puts the end of a failed job's log into the job summary and the most telling
# lines into annotations: both are readable without signing in to GitHub,
# unlike the raw job log.
log="$1"
[ -f "$log" ] || { echo "no log at $log"; exit 0; }
{
  echo "### Failure log (last 250 lines)"
  echo
  echo '```text'
  tail -n 250 "$log" | sed 's/\x1b\[[0-9;]*m//g'
  echo '```'
} >> "$GITHUB_STEP_SUMMARY"
grep -E "^(error|warning: unused)|panicked at|FAILED|failed \(|^xtask: |assertion" "$log" \
  | sed 's/\x1b\[[0-9;]*m//g' | head -n 10 \
  | while IFS= read -r line; do echo "::error title=log::${line}"; done
exit 0
