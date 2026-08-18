# OneTerm documentation index

Which document is the source of truth for what, and which ones are kept only for
history. When a document says **historical** in its header, read it for rationale and
do not copy paths or numbers out of it — the current state is in the code and in the
"current" documents below (`python scripts/check-doc-paths.py` keeps the current
navigation set free of dead paths).

## Start here (current)

| Document | What it owns |
|---|---|
| [`../AGENTS.md`](../AGENTS.md) | Entry point for agents and contributors: required reading, commands, git rules, the CI quality gate. |
| [`architecture.md`](architecture.md) | Architecture index: crate map, dependency direction, service registration, ownership shortcuts. Only current paths. |
| [`PROJECT.md`](PROJECT.md) | Project facts for the Harness flow: purpose, stack, boundaries, invariants, verification commands. |
| [`HARNESS.md`](HARNESS.md) | Documentation-first workflow (Spec Intake → work packet → change → verify → reconcile). Templates in [`templates/`](templates/). |
| [`../README.md`](../README.md) | User-facing README: features, build & run, release packaging. |
| [`../scripts/README.md`](../scripts/README.md) | Every script under `scripts/`, and which ones CI runs. |
| [`../vendor/README.md`](../vendor/README.md) | Vendored forks (`vte`, `alacritty_terminal`, `gpui-component`): provenance, patch model, refresh/check. |
| [`../THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md), [`../NOTICE`](../NOTICE) | Third-party components and licences (generated). |

## Agent guides — `agents/` (current, mandatory reading before code changes)

| Document | What it owns |
|---|---|
| [`agents/structure.md`](agents/structure.md) | Directory tree, crate responsibility table, structure conventions. |
| [`agents/crate-dependency-rules.md`](agents/crate-dependency-rules.md) | Hard crate & dependency rules R1–R12 and their verification commands. |
| [`agents/code-style.md`](agents/code-style.md) | Rust conventions (mandatory). |
| [`agents/dependencies.md`](agents/dependencies.md) | Rev lock (`gpui`, `gpui-component`, vendored forks), allowed auxiliary crates, reference-first research. |
| [`agents/error-policy.md`](agents/error-policy.md) | Runtime error handling and recovery rules. |
| [`agents/persistence.md`](agents/persistence.md) | Persisted files (`terminal.json`, `ui_config.json`, `docks.json`, `ssh_session.json`, …): schema owners and storage mechanics. |
| [`agents/ui-fork-maintenance.md`](agents/ui-fork-maintenance.md) | Maintaining (and eventually retiring) the vendored `gpui-component` patch set. |

## Design records — feature docs

Status is stated in each file's header. "Current" = kept in step with the code;
"historical" = the design as written, later superseded in places.

| Document | Area | Status |
|---|---|---|
| [`terminal-backend.md`](terminal-backend.md) | Terminal backend: sessions, shared pump, event delivery, locking, SSH/local transports | current (2026-08 refresh) |
| [`terminal-split.md`](terminal-split.md) + [`terminal-split/`](terminal-split/) | Split Spaces (right/left/up/down, drag tab into Space) | implemented; index kept current |
| [`auto-completion.md`](auto-completion.md) + [`auto-completion/`](auto-completion/) | Command auto-completion engine, catalogs, overlay, redaction | design spec + implementation plan |
| [`ssh-authentication.md`](ssh-authentication.md) | SSH authentication methods (password, private key, none), key material handling | accepted product contract |
| [`ssh-client-connect.md`](ssh-client-connect.md) | SSH connect flow, host keys, timeouts, keepalive | historical design record (contradictions annotated) |
| [`sftp-browser-design.md`](sftp-browser-design.md) | SFTP browser, transfer queue, `RemotePath` / `TransferHandle` contract | historical + refreshed contract sections |
| [`sftp-follow-terminal-cwd/`](sftp-follow-terminal-cwd/) | SFTP browser follows the terminal CWD (OSC 7) | historical (shipped state summarised in the header) |
| [`auto-update.md`](auto-update.md) | GitHub Releases auto-update: check, download, verify, install, rollback | current (implemented; gaps listed) |
| [`crash-reporting.md`](crash-reporting.md) | Panic / native crash capture and recovery | current |
| [`osc-agent-status.md`](osc-agent-status.md) | OSC 9;7 agent-status proposal (the wire spec) | current |
| [`agent-panel-display.md`](agent-panel-display.md) | Agent Panel model, folding, display rules | current |
| [`osc-sequences-checklist.md`](osc-sequences-checklist.md) | Which OSC sequences OneTerm handles and where | current |
| [`gui-layout.md`](gui-layout.md) | Original workspace layout design (docks, persistence) | historical |
| [`terminal-rendering-optimization.md`](terminal-rendering-optimization.md) | Row cache / damage tracking work | historical |
| [`terminal-fullscreen-perf/`](terminal-fullscreen-perf/) | Full-screen animation performance investigation (DOOM-fire), alacritty fork rationale | historical (implemented) |
| [`terminal-gap-analysis.md`](terminal-gap-analysis.md) | Terminal feature gap analysis | historical |
| [`terminal-semantic-highlighting.md`](terminal-semantic-highlighting.md) | Semantic highlight engine design | historical |
| [`license-analysis.md`](license-analysis.md) | Dependency licence analysis (GPL crates in the Zed graph) | snapshot; policy enforced by `deny.toml` |

## Reviews and remediation

| Document | Status |
|---|---|
| [`review-refresh-2026-08/`](review-refresh-2026-08/) | **Live** review checklist (2026-08-17 refresh) and its phased remediation plan. Tick items here as they land. |
| [`archive/terminal-code-review-remediation-2026-07.md`](archive/terminal-code-review-remediation-2026-07.md) | Pre-restructure terminal review (2026-07-13) — archived; paths no longer exist. |
| [`archive/refactor/ui-crate-restructure.md`](archive/refactor/ui-crate-restructure.md) | The (completed) crate restructure plan — archived. |

When the refresh review is superseded, move it under `archive/` with a status header
and add the new one here.

## Harness records

| Location | Contents |
|---|---|
| [`spec-intakes/`](spec-intakes/) | Spec Intakes (`IN-NNNN`) with their high-level designs and work packets (`US-`/`BUG-`/`WK-`). |
| [`decisions/`](decisions/) | Decision records future work must inherit. |
| [`templates/`](templates/) | Editable templates for intakes, designs, work packets and decisions. |
