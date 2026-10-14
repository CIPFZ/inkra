#!/usr/bin/env python3
"""Reads GitHub Actions results of CIPFZ/inkra without a token (HTML pages).

    gh_runs.py                 recent runs: id, workflow, commit, status
    gh_runs.py <run_id>        status, annotations and job summaries of a run
"""
import html
import re
import sys
import urllib.request

REPO = "CIPFZ/inkra"


def get(path, xhr=True):
    headers = {"User-Agent": "Mozilla/5.0"}
    if xhr:
        headers["X-Requested-With"] = "XMLHttpRequest"
    req = urllib.request.Request(f"https://github.com/{REPO}/{path}", headers=headers)
    return urllib.request.urlopen(req, timeout=60).read().decode("utf-8", "replace")


def text(s):
    s = re.sub(r"<script.*?</script>|<style.*?</style>", "", s, flags=re.S)
    s = re.sub(r"<br\s*/?>|</p>|</div>|</li>|</h\d>|</tr>", "\n", s)
    s = re.sub(r"<[^>]+>", " ", s)
    s = html.unescape(s)
    s = re.sub(r"[ \t]+", " ", s)
    return re.sub(r"\n\s*\n+", "\n", s).strip()


def runs():
    s = get("actions", xhr=False)
    labels = [
        html.unescape(m.group(1))
        for m in re.finditer(r'aria-label="((?:failed|completed successfully|currently running|queued|cancelled|in progress|waiting)[^"]*Run \d+ of[^"]*)"', s)
    ]
    ids = sorted(set(re.findall(r"/actions/runs/(\d+)\"", s)), reverse=True)
    for rid, label in zip(ids, labels):
        print(rid, "|", label[:140])


def run(rid):
    print(text(get(f"actions/runs/{rid}/summary_partial"))[:600])
    print("== annotations")
    print(text(get(f"actions/runs/{rid}/annotations_partial")).replace("Show more\nShow less", "")[:12000])
    page = get(f"actions/runs/{rid}", xhr=False)
    if "Failure log" in page or "summary" in page:
        t = text(page)
        i = t.find("Failure log")
        if i >= 0:
            print("== job summaries")
            print(t[i : i + 20000])


if __name__ == "__main__":
    if len(sys.argv) > 1:
        run(sys.argv[1])
    else:
        runs()
