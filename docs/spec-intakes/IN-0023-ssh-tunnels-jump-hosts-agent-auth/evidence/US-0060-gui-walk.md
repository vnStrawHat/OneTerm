# US-0060 GUI walk (Windows 11, `fast-dev` build, Zed One Dark)

Same method and scratch data as the US-0059 walk, with `"agent_forwarding": true` added to the
scratch `prod-db` session (both scratch files restored afterwards, the launched instance
stopped). No connection was attempted; the request, the bridge, and the refusal path are proven
by the in-process tests in `crates/ssh/src/agent_tests.rs`.

| Screenshot | What it shows |
|---|---|
| `US-0060-session-dialog-agent-forwarding-dark.png` | Property on `prod-db`: the "Forward the SSH agent to the remote host" checkbox between Jump host and Port forwards, checked because the saved session turned it on; a session without the field shows it unchecked. |

Not walked (needs real hosts): the refusal toast in a live terminal, `ssh-add -l` on the remote
host with the switch on and off, and stopping the local agent mid-session.
