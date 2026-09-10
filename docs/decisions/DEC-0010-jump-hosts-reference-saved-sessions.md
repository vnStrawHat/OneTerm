# DEC-0010 Jump hosts reference saved sessions, authenticate per hop, and chain at most four deep

Date: 2026-09-10

## Status

accepted

## Context

IN-0023 adds ProxyJump-style connections. Three shapes were possible for "where does the jump
host come from": a free-text `user@host:port` field on the session, a reference to another
saved session, or a parsed `~/.ssh/config` `ProxyJump` line. The choice decides where the jump
host's authentication preference and host-key trust live, how many secrets the connect dialog
asks for, and whether a chain can loop. OpenSSH also offers `ProxyCommand`, which runs an
arbitrary local program.

## Decision

- A jump host is a **reference to another saved session** (`SshSession.jump_host:
  Option<SshSessionId>`). The hop's host, port, username, and authentication preference are
  those of the referenced session; there is no second copy to keep in step.
- The chain is resolved at save and at connect by following references; a cycle or more than
  `MAX_JUMP_HOPS = 4` hops is rejected with a message. A missing reference fails the connect
  with a message and is cleared on the next save of the dialog.
- **Credentials are entered per hop at connect time** in one dialog, and every hop follows
  DEC 0001 / DEC 0002: nothing secret is persisted, Duplicate Session prompts for every hop
  again, an `Agent` hop needs no input.
- **Host keys are verified per hop** with that hop's own `HostKeyPolicy`; prompts and errors
  name the hop ("jump host for <label>"). Accepting a jump host's key never accepts the
  target's.
- The direct-tcpip channel to the next hop is opened with originator `127.0.0.1:0`; the
  bastion's `AllowTcpForwarding` must permit it, and a refusal is reported as a hop error.
- `ProxyCommand` is **not** supported: OneTerm never spawns a local program from a session
  record.

## Alternatives

- [x] Selected approach described above.
- [ ] Free-text jump field on the session: duplicates username / auth / key path for the same
  bastion across every session that uses it, and needs its own auth preference UI.
- [ ] Parse `~/.ssh/config`: valuable as an import feature but a different intake; it would
  still need a saved-session model to land in.
- [ ] `ProxyCommand`: executing user-supplied commands from a persisted record widens the
  attack surface of `ssh_session.json` for little gain over jump hosts.
- [ ] Unlimited chain depth: a loop would hang the connect; four hops exceeds any real
  bastion topology seen in practice.

## Consequences

- [ ] Benefit to confirm: one bastion saved once serves every session behind it, and changing
  its key path or port updates all of them.
- [ ] Tradeoff: the connect dialog grows one block per hop; a three-hop chain asks for up to
  four secrets in one dialog.
- [ ] Follow-up: an `~/.ssh/config` import intake can map `ProxyJump` onto this reference
  model without a schema change.
