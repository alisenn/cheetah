import ctypes
import http.server
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time

PORT = 8123
PAGES = 10
SNAPSHOTS = [40, 160]
LOAD_TIMEOUT = 90
SITES = [f"http://127.0.0.1:{PORT}/page/{n}" for n in range(PAGES)]

libc = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
libc.responsibility_get_pid_responsible_for_pid.argtypes = [ctypes.c_int]
libc.responsibility_get_pid_responsible_for_pid.restype = ctypes.c_int

LIB = ("var lib=[" + ",".join(str(i * 7919 % 10007) for i in range(60000)) + "];").encode()
PAGE = """<!doctype html><meta charset=utf-8><title>page {n}</title><body><script src="/lib.js?{n}"></script><script>
for(let i=0;i<4000;i++){{const d=document.createElement('div');d.textContent='item '+i+' '+Math.random();document.body.appendChild(d)}}
window.big=[];for(let i=0;i<150000;i++)window.big.push(i*Math.random());
addEventListener('load',()=>fetch('/loaded/{n}'));
</script>"""

loaded = {}
loaded_lock = threading.Lock()


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        body, kind = b"", "text/html"
        if self.path.startswith("/page/"):
            body = PAGE.format(n=self.path.split("/")[2]).encode()
        elif self.path.startswith("/lib.js"):
            body, kind = LIB, "application/javascript"
        elif self.path.startswith("/loaded/"):
            with loaded_lock:
                loaded[self.path.split("/")[2]] = time.time()
        self.send_response(200)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def parse_cpu(text):
    parts = text.split(":")
    seconds = float(parts[-1]) + 60 * int(parts[-2])
    return seconds + (3600 * int(parts[-3]) if len(parts) > 2 else 0)


def processes():
    out = subprocess.run(["ps", "-axo", "pid=,ppid=,rss=,time=,comm="], capture_output=True, text=True).stdout
    rows = []
    for line in out.splitlines():
        fields = line.split(None, 4)
        if len(fields) == 5:
            rows.append((int(fields[0]), int(fields[1]), int(fields[2]), parse_cpu(fields[3]), fields[4]))
    return rows


def members(kind, main):
    rows = processes()
    if kind == "tree":
        keep = {main}
        changed = True
        while changed:
            changed = False
            for row in rows:
                if row[1] in keep and row[0] not in keep:
                    keep.add(row[0])
                    changed = True
        return [r for r in rows if r[0] in keep]
    target = libc.responsibility_get_pid_responsible_for_pid(main)
    return [
        r
        for r in rows
        if r[0] == main
        or (
            ("com.apple.WebKit." in r[4] or "com.apple.Safari" in r[4])
            and libc.responsibility_get_pid_responsible_for_pid(r[0]) == target
        )
    ]


def footprint_mb(pids):
    total = 0.0
    scale = {"B": 1 / 1048576, "KB": 1 / 1024, "MB": 1, "GB": 1024}
    for pid in pids:
        out = subprocess.run(["footprint", "-p", str(pid)], capture_output=True, text=True).stdout
        match = re.search(r"Footprint:\s+([\d.]+)\s+(B|KB|MB|GB)", out)
        if match:
            total += float(match.group(1)) * scale[match.group(2)]
    return total


def launch(browser, profile):
    if browser == "cheetah":
        proc = subprocess.Popen(["target/release/cheetah", *SITES])
        return "resp", proc.pid, proc.kill
    if browser == "chrome":
        binary = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        proc = subprocess.Popen(
            [binary, f"--user-data-dir={profile}", "--no-first-run", "--no-default-browser-check", "--new-window", *SITES]
        )
        return "tree", proc.pid, proc.kill
    subprocess.run(["open", "-a", "Safari", *SITES], check=True)
    for _ in range(30):
        found = subprocess.run(["pgrep", "-x", "Safari"], capture_output=True, text=True).stdout.split()
        if found:
            return "resp", int(found[0]), lambda: subprocess.run(["pkill", "-x", "Safari"])
        time.sleep(1)
    sys.exit("Safari did not start")


def run_once(browser):
    with loaded_lock:
        loaded.clear()
    profile = tempfile.mkdtemp()
    started = time.time()
    kind, pid, stop = launch(browser, profile)
    while time.time() - started < LOAD_TIMEOUT:
        with loaded_lock:
            if len(loaded) >= PAGES:
                break
        time.sleep(0.1)
    with loaded_lock:
        stamps = sorted(loaded.values())
    first = stamps[0] - started if stamps else float("nan")
    last = stamps[-1] - started if len(stamps) >= PAGES else float("nan")
    result = {"first": first, "all": last, "loaded": len(stamps)}
    for at in SNAPSHOTS:
        time.sleep(max(0, started + at - time.time()))
        group = members(kind, pid)
        result[at] = {
            "procs": len(group),
            "rss": sum(r[2] for r in group) / 1024,
            "foot": footprint_mb([r[0] for r in group]),
            "cpu": sum(r[3] for r in group),
        }
    stop()
    time.sleep(3)
    subprocess.run(["pkill", "-f", profile])
    shutil.rmtree(profile, ignore_errors=True)
    return result


def median(values):
    values = [v for v in values if v == v]
    return statistics.median(values) if values else float("nan")


def main():
    browser, repeats = sys.argv[1], int(sys.argv[2])
    server = http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    runs = []
    for index in range(repeats):
        run = run_once(browser)
        runs.append(run)
        print(f"run {index + 1}: {run}", flush=True)
    lines = [
        f"### {browser} ({repeats} runs, median)",
        "",
        "| metric | value |",
        "|---|---|",
        f"| first page loaded (s) | {median([r['first'] for r in runs]):.2f} |",
        f"| all {PAGES} pages loaded (s) | {median([r['all'] for r in runs]):.2f} |",
    ]
    for at in SNAPSHOTS:
        lines += [
            f"| processes @{at}s | {median([r[at]['procs'] for r in runs]):.0f} |",
            f"| footprint MB @{at}s | {median([r[at]['foot'] for r in runs]):.0f} |",
            f"| RSS sum MB @{at}s | {median([r[at]['rss'] for r in runs]):.0f} |",
            f"| CPU seconds @{at}s | {median([r[at]['cpu'] for r in runs]):.1f} |",
        ]
    all_loaded = ", ".join("%.2f" % r["all"] for r in runs)
    footprints = ", ".join("%.0f" % r[SNAPSHOTS[-1]]["foot"] for r in runs)
    lines.append(f"| per-run all-loaded (s) | {all_loaded} |")
    lines.append(f"| per-run footprint MB @{SNAPSHOTS[-1]}s | {footprints} |")
    text = "\n".join(lines) + "\n"
    print(text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as f:
            f.write(text)


main()
