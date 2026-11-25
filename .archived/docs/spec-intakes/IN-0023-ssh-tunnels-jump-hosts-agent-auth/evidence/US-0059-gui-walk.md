# US-0059 GUI walk (Windows 11, `fast-dev` build, Zed One Dark)

Same method as the US-0058 walk (posted messages, `PrintWindow`, scratch `target/ssh_session.json`
and `target/ui_config.json`, both restored afterwards). The scratch `prod-db` session carries three
forwards: `local 127.0.0.1:8080 -> localhost:80`, `remote 0.0.0.0:9000 -> 127.0.0.1:3000`, and
`dynamic 127.0.0.1:1080`. No connection was attempted; the relays are proven by the in-process
tests in `crates/ssh/src/tunnel_tests.rs`.

| Screenshot | What it shows |
|---|---|
| `US-0059-session-dialog-forwards-dark.png` | Property on `prod-db`: the "Port forwards" field between Jump host and Group with one row per saved forward (kind select, bind, port, `->`, target host, port, remove), the "This listener is reachable from other machines." caution under the `0.0.0.0` remote row, the Dynamic row hiding the target and showing `SOCKS5`, and the `+ Add` button. The dialog is 560 px wide so every input is legible. |

Known ceiling (see the `ponytail:` note in `session_dialog.rs`): `FormDialog` does not scroll, so on
this 926 px tall capture the footer buttons are already cut at the bottom edge; a 1080p work area
fits about five forward rows.

Not walked (needs real hosts): the bind-failure toast in the terminal, `curl` through a local,
remote, and SOCKS5 forward, and `netstat` after closing the tab.
