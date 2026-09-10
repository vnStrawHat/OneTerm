# DEC-0011 Forwarding defaults: loopback binds, warn on bind failure, SOCKS5 CONNECT only, agent forwarding opt-in per session

Date: 2026-09-10

## Status

accepted

## Context

IN-0023 adds port forwards (local, remote, dynamic) and SSH agent forwarding. Each opens a
listening socket or hands a capability to the remote host, so the defaults decide what an
unattended OneTerm exposes. Two behaviours also needed a rule: what happens when a forward
cannot start (a port already in use is routine after Duplicate Session or a crashed tab), and
how much of SOCKS to implement without a new dependency.

## Decision

- **Bind addresses default to loopback.** `Local` and `Dynamic` forwards bind `127.0.0.1`,
  `Remote` forwards ask the server for `127.0.0.1`, unless the user types another address;
  the dialog shows a one-line caution for a non-loopback bind. OneTerm never binds `0.0.0.0`
  on its own.
- **A forward that cannot start does not fail the connection.** Bind errors and refused
  `tcpip-forward` requests each produce one warning notification naming the address; the
  remaining forwards still start and the shell opens. A silent skip is not allowed.
- **Dynamic forwards are SOCKS5, no authentication, CONNECT only** (RFC 1928 with IPv4,
  IPv6, and domain addresses). BIND and UDP ASSOCIATE answer "command not supported"; a
  client that offers no `no-auth` method is refused. SOCKS4/4a is not implemented. Domain
  names are resolved on the remote side.
- **Agent forwarding is off by default and per session.** The request is sent only when the
  session's switch is on; the handler closes any agent channel the server opens while the
  switch is off, so a server cannot obtain agent access that the user did not grant. A
  refused request is a warning, not a failure.
- **Every forward and bridge dies with the session** through the session's cancellation
  token, before the transport disconnects; no listener outlives its tab.

## Alternatives

- [x] Selected approach described above.
- [ ] Abort the connection when a forward cannot start: turns a stale port from a duplicated
  session into a failed login and forces the user to edit the session before reconnecting.
- [ ] Bind `0.0.0.0` by default (as some GUI clients do): exposes forwarded services to the
  LAN without the user asking; loopback matches OpenSSH's default.
- [ ] Full SOCKS5 with authentication and UDP, or a SOCKS crate: no consumer needs it for a
  browser or `curl` through SSH, and it would add a dependency for a few dozen lines.
- [ ] A global "forward agent" setting: makes the most sensitive capability the easiest to
  forget; per session keeps the grant visible next to the host it applies to.

## Consequences

- [ ] Benefit to confirm: Duplicate Session of a forwarding session still opens a shell, with
  a visible warning about the ports it could not take.
- [ ] Tradeoff: users who want LAN-reachable forwards must type the bind address each time;
  there is no "share on the network" shortcut.
- [ ] Follow-up: a status indicator for active tunnels was left out of IN-0023; add it when
  the warning toast alone proves insufficient.
