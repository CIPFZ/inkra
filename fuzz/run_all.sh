#!/usr/bin/env bash
# Runs every fuzz target for SECONDS (default 2 h) in parallel; logs in logs/.
set -u
cd "$(dirname "$0")"
secs="${1:-7500}"
mkdir -p logs
for t in markdown_parse markdown_incremental html_to_markdown math diagram; do
  cargo +nightly fuzz run "$t" -- -max_total_time="$secs" -rss_limit_mb=4096 -max_len=65536 -timeout=10 > "logs/$t.log" 2>&1 &
done
wait
for t in markdown_parse markdown_incremental html_to_markdown math diagram; do
  echo "== $t"; grep -E "^#[0-9]+.*DONE|Done [0-9]+ runs|SUMMARY|panicked|ERROR" "logs/$t.log" | tail -3
done
ls artifacts/*/ 2>/dev/null
