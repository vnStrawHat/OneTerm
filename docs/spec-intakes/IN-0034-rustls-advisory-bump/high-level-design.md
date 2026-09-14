# High-Level Design: Patch-level rustls bump for RUSTSEC-2026-0285

Intake: IN-0034
Lane: normal
Date: 2026-09-14

## Idea

Nothing in the workspace declares `rustls`; it is resolved transitively from `reqwest`, whose
requirement is `^0.23`. The patched release `0.23.45` satisfies that requirement, so the whole
fix is a `Cargo.lock` re-resolution: `cargo update -p rustls --precise 0.23.45`. No manifest
change, no `deny.toml` ignore entry, no dependant bump.

`THIRD-PARTY-NOTICES.md` is generated from `Cargo.lock`, so it is regenerated in the same
change to keep `scripts/third-party-notices.py --check` green.

## Diagram

```text
Cargo.toml  reqwest = "0.12" (rustls-tls-native-roots)   <- unchanged
                    |
                    v
Cargo.lock  rustls 0.23.40         --cargo update-->  rustls 0.23.45
            rustls-webpki 0.103.13 --------------->   rustls-webpki 0.103.15
                    |
                    v
            THIRD-PARTY-NOTICES.md --regenerate-->  same licences, new versions
```

## UI Wireframe

N/A — no UI surface. The change is confined to the resolved dependency graph.

## Data Flow

1. `cargo deny check advisories` reads `Cargo.lock` and matches it against the RustSec database.
2. The lock pins the patched rustls; the advisory no longer matches.
3. `scripts/third-party-notices.py` re-reads the same lock and rewrites the notices table.

## Detail Design

- [ ] Detail design: not needed
- Reason: a lockfile re-resolution plus a regenerated generated file; there is no code to shape.
