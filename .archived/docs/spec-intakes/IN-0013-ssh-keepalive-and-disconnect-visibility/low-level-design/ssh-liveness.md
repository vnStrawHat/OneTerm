# Low-Level Design: ssh-liveness

Intake: IN-0013
HLD: high-level-design.md
Topic: ssh-liveness
Date: 2026-08-21

## Concern

Persist, validate, and apply a process-global SSH keepalive policy to newly opened SSH sessions without coupling UI crates to the SSH backend.

## Design

- Add `SshConfigGroup` to `TerminalConfig` with serde defaults:
  - `keepalive_enabled: bool = true`
  - `keepalive_interval_secs: u64 = 30`
  - `keepalive_max: usize = 3`
- Normalize the interval to `5..=3600` seconds and `keepalive_max` to `1..=20`. Settings number fields and persistence use the same bounds.
- Mirror the group in `TerminalSettings` and preserve it through `from_config`/`to_config`.
- Add a small runtime `SshKeepaliveConfig` value in the backend-neutral terminal/core boundary rather than exposing `Duration` or russh types to UI code.
- Extend only `SessionFactory::connect_ssh`; local-shell creation remains unchanged.
- Snapshot the policy in `connect_ssh_session` alongside scrollback and logging. Existing sessions are intentionally not reconfigured.
- In `oneterm-ssh::connect`, map enabled to `Some(Duration::from_secs(interval))` and disabled to `None`, and map the captured normalized max directly to russh `keepalive_max`.

## Interfaces

```rust
pub struct SshKeepaliveConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub max: usize,
}

fn SessionFactory::connect_ssh(
    &self,
    cfg: SshConfig,
    size: PtySize,
    scrollback: usize,
    security: TerminalSecurityPolicy,
    logging: TerminalLogConfig,
    keepalive: SshKeepaliveConfig,
) -> Result<Box<dyn TerminalSession>>;
```

Persisted fragment:

```json
{
  "ssh": {
    "keepalive_enabled": true,
    "keepalive_interval_secs": 30,
    "keepalive_max": 3
  }
}
```

## Edge Cases and Failure Modes

- [x] Missing `ssh` group or fields load the 0.4.2-compatible enabled/30-second default.
- [x] Interval values below 5 or above 3600 are normalized before becoming live runtime settings.
- [x] Missing max uses three; max values outside `1..=20` are normalized before runtime and persistence.
- [x] Disabled keepalive maps to russh `keepalive_interval = None` without changing authentication or channel setup.
- [x] A settings change does not mutate already connected sessions.
- [x] Persistence read failures retain the existing `persist_blocked` protection.

## Verification

- [x] Config serde tests cover absent fields, defaults, and out-of-range normalization.
- [x] Settings/config roundtrip includes the SSH group.
- [x] Runtime policy tests cover enabled/disabled interval mapping and normalized max mapping.
- [x] SessionFactory implementations and test doubles compile with the new argument.
