# US-0058 GUI walk (Windows 11, `fast-dev` build, Zed One Dark)

Driven with posted `WM_KEYDOWN` / `WM_CHAR` / mouse messages and captured with
`PrintWindow(hwnd, hdc, 2)` (the same method as the US-0056 walk). The debug config directory
is `target/`: its `ssh_session.json` was replaced by three scratch sessions — `bastion`
(`ops@bastion.example.com:22`, private key), `prod-db` (`deploy@10.0.5.20:22`, password,
`jump_host = bastion`), and `agent-host` (`deploy@10.0.5.21:22`, SSH agent) — and
`ui_config.json` bound `f5` = New SSH Session so Quick Connect opens from a plain key. Both files
were restored afterwards and the launched instance was stopped. No connection was attempted:
the hosts do not exist, and the route itself is proven by the in-process two-server tests in
`crates/ssh/src/route_tests.rs`.

| Screenshot | What it shows |
|---|---|
| `US-0058-quick-connect-agent-radio-dark.png` | Quick Connect with the new rows: "Jump host [None v]" between Username and Authentication, and the three-way radio Password / Private Key / SSH Agent. |
| `US-0058-quick-connect-jump-dropdown-dark.png` | The Jump host dropdown open: a search box and the three saved sessions as `label  (user@host:port)`. |
| `US-0058-quick-connect-hop-block-dark.png` | After picking `bastion`: a "Jump host bastion / ops@bastion.example.com:22" block with its own Private Key path and Passphrase field appears above the target fields; the picker shows the choice with a clear (x) button; the target keeps its own Password radio. |
| `US-0058-connect-dialog-hop-dark.png` | Double-clicking `prod-db` opens "Connect to prod-db (deploy@10.0.5.20:22)": the bastion hop block first (its passphrase field has the initial focus), a divider, the `ssh://deploy@10.0.5.20:22` banner, then the target's Password field. |
| `US-0058-session-dialog-jump-picker-dark.png` | Property on `prod-db`: the Edit SSH Session dialog shows "Jump host: bastion (ops@bastion.example.com:22)" between Authentication and Group, prefilled from the saved `jump_host` reference. |

Not walked (needs real hosts): the hop-attributed host-key prompt, the "jump host ... refused a
direct-tcpip channel" error text, and Duplicate Session of a jump-host session.
