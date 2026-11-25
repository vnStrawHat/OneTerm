# Low-Level Design: Agent Panel and OSC 20308 removal

Intake: IN-0049
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: what is deleted, and how documents written by an older OneTerm still load
Date: 2026-10-15

## Concern

Delete the Agent Panel feature and OneTerm's own agent channel with no user-visible residue,
while every `ui_config.json` and `docks.json` an older OneTerm wrote keeps loading without a
quarantine, a prompt, or a "unknown panel" placeholder.

## Design

### Deleted outright

| Area | Items |
| --- | --- |
| Crate | `crates/agent-ui` (`oneterm-agent-ui`): workspace member, workspace dependency, `oneterm-app` dependency, `init` call, `scripts/dependency-graph-policy.json` entries |
| `oneterm-state` | `agent_registry.rs`, `agent_model.rs` (+ tests), `agent_focus.rs`; the `AgentFocuser` field and parameter of `AppServices::install`; `panel_names::AGENT` and its `RightDockMode::Agent` arm |
| `oneterm-terminal` | `osc_agent/` (types, dedup, receiver tests); the 20308 and `9;7` halves of `osc.rs`; the two routes in `adapter_config`; `SessionEvent::AgentStatus`; `SharedState::last_agent_seq` and the three agent counters; the agent arms of `OscRouter`; the re-exports in `lib.rs` |
| `oneterm-terminal-view` | `agent.rs`, `terminal_view/agent_status.rs`, `deps.agent_registry`, the rename mirror in `tab_title.rs`, the `AgentStatus` arm in the view, the `agent_focuser` export; `TerminalPanel` helpers left with no caller (`tab_label_with_title`, `space_order`, `tab_panel_weak`, `SpaceTree` ordering helpers) if the compiler reports them dead |
| `oneterm-workspace` | the "Agent" segment of the title-bar control; `RightDockMode::Agent` handling in `actions.rs` and `mod.rs` (`right_dock_panel_mode` reduces to "is the SSH Client panel"); `panel_names::AGENT` in `persistence.rs` and `dock_skin.rs` |
| `oneterm-settings` | `UiConfig::agent_stale_threshold_ms`, its constant and accessor |
| `oneterm-app` | the `oneterm_agent_ui::init` call and the focuser argument |
| Scripts | `scripts/agent-status-demo.sh`, `.ps1`, their row in `scripts/README.md`; the agent-ui note in `check-theme-contrast.py` |

### Kept, with a changed meaning

- **`RightDockMode`** keeps `SshClient` and `None`. `None` gains `#[serde(alias = "agent")]`,
  so a saved `"right_dock_mode": "agent"` deserializes as `None` (owner ruling) and is
  written back as `"none"` on the next save. No schema version bump: the document's shape is
  unchanged, and an older OneTerm reading `"none"` already understands it.
- **`agent_stale_threshold_ms`** in an old `ui_config.json` is an unknown field. `UiConfig` has no
  `deny_unknown_fields`, so it is ignored on read and dropped on the next write. Confirmed by
  a test, not assumed.
- **`docks.json`**: a right dock saved as `agent_panel` is replaced at start-up, because
  `reset_center_only` always installs the SSH Client panel on the right; the persisted `None`
  then collapses it. The centre is always reset, so an Agent panel dragged into the centre is
  dropped too. A `zoomed_panel` of `"agent_panel"` finds no tab and is ignored. No migration
  step and no schema bump.
- **OSC 9;7** stops being special. The engine already turns OSC 9 into a notification; a
  stray `9;7;...` from an old emitter becomes an ordinary (sanitized, rate-limited) toast. No
  known third-party emitter used the alias.
- **OSC 20308** returns to the engine's default `Drop` route: parsed, counted as unhandled,
  discarded. The `20308;0` probe goes unanswered, which is what an emitter reads as
  "unsupported".

### Docs

- `docs/osc-agent-status.md` and `docs/agent-panel-display.md` move to `docs/archive/` with a
  "withdrawn" header naming `DEC-0022`; links to them from current docs are removed or
  repointed. Accepted decisions keep their historical references.
- Updated: `README.md`, `AGENTS.md` (project introduction), `docs/PROJECT.md`, `docs/README.md`,
  `docs/agents/structure.md`, `docs/agents/persistence.md` (the alias and the dropped field),
  `docs/architecture.md`, `docs/gui-layout.md`, `docs/terminal-backend.md`,
  `docs/osc-sequences-checklist.md`, `crates/vt/docs/guide/05-osc.md` (if it uses 20308 as its
  forwarding example, the example changes to 7501).

## Interfaces

```rust
// oneterm-core
pub enum RightDockMode {
    #[default]
    SshClient,
    #[serde(alias = "agent")]
    None,
}

// oneterm-state
impl AppServices {
    pub fn install(
        cx: &mut App,
        session_factory: Arc<dyn SessionFactory>,
        workspace_commands: WorkspaceCommands,
        active_terminal_metrics: ActiveTerminalMetricsProvider,
    ) -> Result<(), &'static str>;
}
```

## Edge Cases and Failure Modes

- [x] `ui_config.json` with `"right_dock_mode": "agent"` loads, the dock is hidden, and the next
  save writes `"none"`.
- [x] `ui_config.json` with `"agent_stale_threshold_ms": 42` loads without quarantine.
- [ ] `docks.json` whose right dock is `agent_panel` loads; the right dock holds the SSH Client
  panel (collapsed under the persisted mode) and no `InvalidPanel` placeholder survives the
  next save.
- [ ] An elevated window: unchanged (it reads no `docks.json` and has no right dock).
- [ ] `OSC 20308;1;...` and `OSC 20308;0` produce no reply and no event.

## Verification

- [x] `oneterm-settings` tests: the two `ui_config.json` cases above.
- [ ] `oneterm-workspace` test: loading a `docks.json` fixture with an `agent_panel` right dock
  leaves no panel named `agent_panel` in the dock area.
- [ ] `oneterm-terminal` test: a 20308 probe yields no transport write.
- [ ] `grep -rn "20308\|agent_panel\|AgentRegistry\|agent-ui" crates scripts` finds nothing
  outside comments that name the withdrawal.
- [ ] `verify-dependency-graph.py`, `check-doc-paths.py`, the full `ci-local.sh`.
