import ctypes
import os
import shutil
import subprocess
import sys
import tempfile
import time

SITES = [
    "https://en.wikipedia.org/wiki/Rust_(programming_language)",
    "https://news.ycombinator.com",
    "https://github.com/rust-lang/rust",
    "https://www.bbc.com",
    "https://stackoverflow.com",
    "https://developer.mozilla.org",
    "https://www.theverge.com",
    "https://www.nytimes.com",
    "https://www.cnn.com",
    "https://www.amazon.com",
]
SNAPSHOTS = [45, 165]

libc = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
libc.responsibility_get_pid_responsible_for_pid.argtypes = [ctypes.c_int]
libc.responsibility_get_pid_responsible_for_pid.restype = ctypes.c_int


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


def launch(browser, profile):
    if browser == "cheetah":
        proc = subprocess.Popen(["target/release/cheetah", *SITES])
        return "resp", proc.pid, lambda: proc.kill()
    if browser == "chrome":
        binary = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        proc = subprocess.Popen(
            [binary, f"--user-data-dir={profile}", "--no-first-run", "--no-default-browser-check", "--new-window", *SITES]
        )
        return "tree", proc.pid, lambda: proc.kill()
    subprocess.run(["open", "-a", "Safari", *SITES], check=True)
    for _ in range(30):
        found = subprocess.run(["pgrep", "-x", "Safari"], capture_output=True, text=True).stdout.split()
        if found:
            return "resp", int(found[0]), lambda: subprocess.run(["pkill", "-x", "Safari"])
        time.sleep(1)
    sys.exit("Safari did not start")


def main():
    browser = sys.argv[1]
    profile = tempfile.mkdtemp()
    kind, pid, stop = launch(browser, profile)
    started = time.time()
    lines = []
    for at in SNAPSHOTS:
        time.sleep(max(0, started + at - time.time()))
        group = members(kind, pid)
        rss_mb = sum(r[2] for r in group) // 1024
        cpu_s = sum(r[3] for r in group)
        line = f"| {browser} | {at}s | {len(group)} | {rss_mb} | {cpu_s:.1f} |"
        print(line, flush=True)
        lines.append(line)
    stop()
    shutil.rmtree(profile, ignore_errors=True)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as f:
            f.write("| browser | after | processes | RSS MB (sum) | CPU seconds (sum) |\n|---|---|---|---|---|\n")
            f.write("\n".join(lines) + "\n")


main()
