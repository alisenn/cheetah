#!/bin/sh
set -e
cd "$(dirname "$0")/.."
curl -fsS -o src/blocklist.txt "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=hosts&showintro=0&mimetype=plaintext"
wc -l src/blocklist.txt
