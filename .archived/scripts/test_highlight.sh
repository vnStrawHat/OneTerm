#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────
#  Semantic Highlight Test Script
#  Prints sample output covering all 18 highlight classes
# ─────────────────────────────────────────────────────────────

set -u

# ── Prompt lines & commands ──────────────────────────────────
echo ""
echo "════════ Prompt & Command ════════"
echo '$ ls -la --color=auto /home/user'
echo '$ git commit -m "fix: handle edge case" --amend'
echo '# systemctl status nginx'
echo '> echo "hello world"'

# ── Options / flags ──────────────────────────────────────────
echo ""
echo "════════ Options & Flags ════════"
echo '$ curl -X POST -H "Content-Type: application/json" -d "{\"key\":\"value\"}" https://api.example.com'
echo '$ grep -rn --include="*.rs" "TODO" src/'
echo '$ cargo build --release --all-features 2>&1 | tee build.log'

# ── Error / Success / Warn / Info / Debug ───────────────────
echo ""
echo "════════ Log Levels ════════"
echo "error: failed to connect to database at 10.0.0.5:5432"
echo "ERROR: permission denied for user 'admin' on table 'users'"
echo "success: deployment completed in 12.3s"
echo "SUCCESS: 3 files uploaded, 0 failures"
echo "warning: deprecated API usage in src/main.rs:42"
echo "WARN: high memory usage 89% detected"
echo "info: listening on 0.0.0.0:8080"
echo "INFO: request processed in 156ms"
echo "debug: cache hit ratio = 0.87, miss = 13"
echo "DEBUG: retrying connection (attempt 3/5)"

# ── Paths ───────────────────────────────────────────────────
echo ""
echo "════════ Paths ════════"
echo "$ cat /etc/nginx/conf.d/default.conf"
echo "Copying /usr/local/bin/tool -> /opt/bin/tool"
echo "Updated ~/.config/oneterm/settings.json"
echo "Found 3 matches in src/views/terminal/mod.rs:128"

# ── IPv4 / IPv6 ──────────────────────────────────────────────
echo ""
echo "════════ Network Addresses ════════"
echo "Pinging 192.168.1.1 with 32 bytes of data:"
echo "Connecting to 10.0.0.42:22 ..."
echo "IPv6 link-local: fe80::1c2d:3e4f:5a6b%eth0"
echo "Resolved example.com -> 2607:f8b0:4004:80a::200e"

# ── MAC addresses ──────────────────────────────────────────
echo ""
echo "════════ MAC Addresses ════════"
echo "eth0: link/ether aa:bb:cc:dd:ee:ff brd ff:ff:ff:ff:ff:ff"
echo "WiFi adapter MAC: 00:1A:2B:3C:4D:5E"
echo "Assigned MAC: 3C:5A:B8:01:2D:4F"

# ── Date / Time ──────────────────────────────────────────────
echo ""
echo "════════ Date & Time ════════"
echo "2024-01-15 09:30:45 INFO  Server started"
echo "12/25/2023 11:59 PM - session ended"
echo "Last login: Wed Oct 25 10:15:30 UTC 2023"
echo "Build finished at 2024-03-01T14:22:08.123Z"

# ── Numbers ─────────────────────────────────────────────────
echo ""
echo "════════ Numbers ════════"
echo "Throughput: 1_048_576 bytes/sec, latency: 2.5ms"
echo "Memory: 4096 MB free of 16384 MB total"
echo "Loaded 0x7FFE_0000 entries, ratio 3.14159"

# ── Strings ─────────────────────────────────────────────────
echo ""
echo "════════ Strings ════════"
echo 'status = "running"'
echo "name = \"highlight-engine\""
echo "path = '/usr/local/bin'"
echo "query: \"SELECT * FROM users WHERE active = 1\""

# ── Operators & Brackets ────────────────────────────────────
echo ""
echo "════════ Operators & Brackets ════════"
echo "result = (a + b) * 2 / 3 % 4"
echo 'if [ $count -gt 10 ]; then echo "big"; fi'
echo "config = { \"port\": 8080, \"host\": \"0.0.0.0\" }"
echo "array = [1, 2, 3] + [4, 5] -> [1, 2, 3, 4, 5]"

# ── URLs ────────────────────────────────────────────────────
echo ""
echo "════════ URLs ════════"
echo "Downloading https://github.com/user/repo/archive/main.zip ..."
echo "Docs: https://docs.example.com/v2/getting-started"
echo "Repository: https://gitlab.com/team/project.git"
echo "File:// protocol not supported, use http://localhost:3000"

# ── Permissions (ls -l style) ───────────────────────────────
echo ""
echo "════════ Permission Blocks ════════"
echo "drwxr-xr-x  4 user user  4096 Jan 15 10:30 src"
echo "-rwxr-xr-x  1 user user  1234 Jan 15 10:31 build.sh"
echo "-rw-r--r--  1 user user 5678 Jan 15 09:00 README.md"
echo "crw-rw-rw-  1 root root 1, 3 Jan 15 08:00 /dev/null"

# ── Mixed / realistic output ───────────────────────────────
echo ""
echo "════════ Realistic Mixed Output ════════"
echo "=== Build Log 2024-01-15 14:30:00 ==="
echo "info: compiling oneterm v0.2.0 (/home/user/oneterm)"
echo 'warning: unused import: `std::collections::HashMap`'
echo "  --> src/main.rs:15:5"
echo "success: built in 8.42s"
echo ""
echo '$ ./target/debug/oneterm --port 8080 --host 0.0.0.0'
echo "debug: binding to 0.0.0.0:8080"
echo "info: 1 client connected from 192.168.1.100 (MAC: aa:bb:cc:11:22:33)"
echo "error: connection reset by peer at 2024-01-15T14:31:22Z"
echo "   url: https://api.example.com/v1/status"
echo "   retry 3/5 in 2.0s ..."
echo "success: reconnected, latency 4.2ms"

# ── Prompt variants ─────────────────────────────────────────
echo ""
echo "════════ Prompt Variants ════════"
echo 'user@host:~$ ./run.sh --flag value 2>&1'
echo 'root@server:/# systemctl restart sshd'
echo 'C:\Users\Admin> dir /s /b *.log'
echo 'PS C:\\> Get-Process | Where-Object {$_.CPU -gt 10}'

echo ""
echo "════════ Done ════════"