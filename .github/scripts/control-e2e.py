#!/usr/bin/env python3
"""CM-003 end to end: `inkra --control 0` driven by `inkra-cli mcp --attach`.

    control-e2e.py <inkra> <inkra-cli>   (needs a display, e.g. xvfb-run)
"""
import base64, glob, json, os, subprocess, sys, tempfile, time

inkra, cli = sys.argv[1], sys.argv[2]
cfg = tempfile.mkdtemp(prefix="inkra-e2e-")
doc = os.path.join(cfg, "note.md")
with open(doc, "w") as f:
    f.write("# Note\n\nbody\n")
app = subprocess.Popen([inkra, "--control", "0", "--config", cfg, "--no-session", "--new"])
try:
    for _ in range(300):
        if glob.glob(os.path.join(cfg, "data", "control", "*.json")):
            break
        if app.poll() is not None:
            sys.exit(f"inkra exited with {app.returncode}")
        time.sleep(0.1)
    else:
        sys.exit("no connection file after 30 s")
    mcp = subprocess.Popen([cli, "mcp", "--attach", "--config", cfg],
                           stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    n = 0
    def rpc(method, params=None):
        global n
        n += 1
        mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "id": n, "method": method, "params": params or {}}) + "\n")
        mcp.stdin.flush()
        r = json.loads(mcp.stdout.readline())
        if "error" in r:
            sys.exit(f"{method}: {r['error']}")
        return r["result"]
    def tool(name, **args):
        r = rpc("tools/call", {"name": name, "arguments": args})
        if r.get("isError"):
            sys.exit(f"{name}: {r['content'][0]['text']}")
        c = r["content"][0]
        return c if c["type"] == "image" else json.loads(c["text"])
    print("initialize:", rpc("initialize", {"protocolVersion": "2025-06-18"})["serverInfo"])
    tool("inkra_open", path=doc)
    tool("inkra_execute", command="edit.insertText", params={"text": "X"})
    tool("inkra_key", text="Y")
    tool("inkra_key", key="End")
    text = tool("inkra_inspect", include=["text"])["text"]
    assert "X" in text and "Y" in text, text
    tool("inkra_save")
    saved = open(doc).read()
    assert "X" in saved and "Y" in saved, saved
    shot = tool("inkra_screenshot")
    png = base64.b64decode(shot["data"])
    assert png[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    print(f"screenshot: {len(png)} bytes")
    log = open(os.path.join(cfg, "data", "logs", "inkra.log")).read()
    assert "control: injected text of 1 characters" in log, "injection not logged"
    assert "Y\n" not in log.split("control: injected")[1][:40], "typed text in the log"
    mcp.stdin.close()
    mcp.wait(timeout=10)
    print("control e2e: OK")
finally:
    app.terminate()
    try:
        app.wait(timeout=10)
    except subprocess.TimeoutExpired:
        app.kill()
# A killed instance leaves its file; the next client removes it.
r = subprocess.run([cli, "mcp", "--attach", "--config", cfg], input="", capture_output=True, text=True)
assert r.returncode == 2 and "no running Inkra" in r.stderr, r.stderr
assert not glob.glob(os.path.join(cfg, "data", "control", "*.json")), "stale connection file kept"
print("stale connection file removed: OK")
