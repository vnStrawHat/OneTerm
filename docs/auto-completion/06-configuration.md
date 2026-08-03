# 06 — Configuration & gating

> Part of [Terminal auto-completion design](../auto-completion.md). The
> `terminal.json` settings group, the OSC 133 / alternate-screen gating that turns
> completion off inside TUIs, and the Settings UI surface.

## 1. Where settings live

Completion settings are a new group in the existing `terminal.json` schema, owned
by `crates/settings/src/terminal_config/` (a new `completion.rs` module alongside
`font.rs`, `cursor.rs`, `security.rs`, …) and mirrored in the live
`TerminalSettings`. This matches every other terminal setting group and reuses the
existing load/save + live-apply plumbing. See
[`docs/agents/persistence.md`](../agents/persistence.md) for schema ownership rules.

> Note: the **history data** (`memory` source) is *not* persisted — only the
> *settings* that control it are. `max_history` is a knob; the recorded commands
> themselves live only in RAM ([01](01-architecture.md) §4).

## 2. Settings schema (`completion` group)

```jsonc
// terminal.json (excerpt)
{
  "completion": {
    "enabled": true,                 // master on/off for auto-completion
    "accept_tab": true,              // Tab accepts the selection (else Tab → shell)
    "max_history": 500,              // per-family in-session history capacity (0 = disable history)
    "min_prefix_len": 1,             // min chars before command suggestions appear
    "max_visible_items": 8,          // rows shown in the overlay before scrolling
    "sources": {                     // per-source toggles
      "memory": true,
      "manual": true,
      "external": true
    },
    "fuzzy": true,                   // allow fuzzy (subsequence) matches as a secondary pass
    "inherit_ancestor_options": true,// in subcommand trees, also offer ancestor options (ranked lower) — see 10
    "disable_in_alt_screen": true,   // suppress inside the alternate screen (TUIs) — see §3
    "require_prompt_region": true,   // only show inside the OSC 133 command-input region — see §3
    "windows_allow_coreutils": false,// let Cmd/PowerShell also suggest coreutils+linux commands — see 02 §4.1
    "force_family": null,            // null | "cmd" | "powershell" | "unix" — override shell detection
    "redact_sensitive": true         // strip secret values before recording history (see 08) — keep true
  }
}
```

Defaults are chosen so the feature is **on and safe** out of the box. Field
semantics:

| Field | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Master switch. `false` → no overlay, no history capture. |
| `accept_tab` | `true` | Whether `Tab` accepts (the request's "enable/disable accept tab"). |
| `max_history` | `500` | Per-family ring capacity. `0` disables the `memory` source and clears it. |
| `min_prefix_len` | `1` | Chars required before command suggestions (matches the "type `d`" example). Option context ignores this — a lone trigger shows options. |
| `max_visible_items` | `8` | Visible overlay rows. |
| `sources.*` | all `true` | Turn individual sources off. |
| `fuzzy` | `true` | Secondary fuzzy matching. |
| `inherit_ancestor_options` | `true` | In subcommand trees, also suggest ancestor options (ranked below the active node's own). [10](10-subcommands.md) §3.2. |
| `disable_in_alt_screen` | `true` | See §3. |
| `require_prompt_region` | `true` | See §3. |
| `windows_allow_coreutils` | `false` | Let the `Cmd`/`PowerShell` families also search the `coreutils` + `linux` categories (Git-Bash/busybox users). Appended at lowest precedence — [02](02-data-sources.md) §4.1. |
| `force_family` | `null` | Override the detected shell family ([03](03-shell-detection.md) §5). |
| `redact_sensitive` | `true` | Keep secrets out of history ([08](08-security-redaction.md)). Not recommended to disable. |

All fields use `#[serde(default = …)]` so old `terminal.json` files (without the
`completion` group) load with defaults — same pattern as `SecurityConfig`.

## 3. Gating: OSC 133 + alternate screen (TUI-safe)

Auto-completion must never fire while a full-screen TUI (`vim`, `htop`, `less`,
`fzf`, a pager, an SSH TUI…) is running, and should only appear when the user is
actually editing a command at the prompt. Two independent gates, both default-on:

### 3.1 Alternate screen (`disable_in_alt_screen`)

Full-screen programs switch the terminal to the **alternate screen** (DECSET
`?1049h` / `?47h`). The alacritty engine already tracks this mode. When the active
terminal is on the alternate screen, the controller:

- **Suppresses** new overlays and **dismisses** any visible one.
- **Pauses** history capture (keystrokes in a TUI are not shell commands).

On return to the primary screen (`?1049l`), completion resumes.

### 3.2 Command-input region (`require_prompt_region`)

Even on the primary screen, suggestions should only appear when the cursor is in
the **command-input region** — between OSC 133 `PromptEnd` (`B`) and `OutputStart`
(`C`). OneTerm already classifies rows via `RowRole`
(`Output`/`Prompt`/`Command`, `crates/highlight/src/role.rs`). The controller shows
the overlay only when the cursor sits on a `RowRole::Command` row.

Fallback when the shell emits **no** OSC 133: `require_prompt_region` degrades to a
best-effort check (cursor not inside a region known to be program output). If the
signal is unavailable, the gate is treated as "at prompt" so the feature still
works on shells without integration — at the cost of possibly appearing during a
program that reads a line of input. Users on such shells can set
`require_prompt_region: false` or rely on `disable_in_alt_screen` (which covers the
common full-screen cases).

### 3.3 Interaction summary

```
show overlay  ⇔  enabled
              ∧  ¬(disable_in_alt_screen ∧ on_alt_screen)
              ∧  (¬require_prompt_region ∨ cursor_in_command_region)
              ∧  engine.suggest(...) is non-empty
```

## 4. Live apply

Changing settings takes effect without restarting terminals (same mechanism as
other `TerminalSettings`):

- `enabled` / `accept_tab` / `max_visible_items` / `min_prefix_len` / `sources` /
  `fuzzy` / gating flags: applied on the next keystroke.
- `max_history`: resizes the ring immediately; lowering it evicts oldest entries;
  `0` clears the `memory` store.
- `windows_allow_coreutils` / `force_family`: re-derive the category search path /
  family on the next suggestion.

## 5. Settings UI

`crates/settings-ui` adds a **Completion** section to the Terminal settings page:

- Toggle: Enable auto-completion (`enabled`).
- Toggle: Accept with Tab (`accept_tab`).
- Number: Max command history (`max_history`).
- Number: Min characters before suggesting (`min_prefix_len`).
- Number: Visible suggestions (`max_visible_items`).
- Toggles: Sources (History / Manual / External).
- Toggle: Fuzzy matching.
- Toggle: Disable inside full-screen apps (`disable_in_alt_screen`).
- Toggle: Allow coreutils on Windows (`windows_allow_coreutils`).
- Button: Clear session history (calls `CompletionHistory::clear`).
- (Advanced) Force shell family.

Controls read/write `TerminalSettings` through the existing settings-ui plumbing;
no new persistence path is introduced.

## 6. Actions / key bindings

Expose `oneterm-actions` action structs so users can bind keys (via the existing
key-binding UI):

- `ToggleCompletion` — flip `enabled` for the session/global.
- `ClearCompletionHistory` — clear the `memory` store.
- `TriggerCompletion` — force-open the overlay at the cursor even below
  `min_prefix_len` (manual trigger). **Default binding: `Ctrl+Shift+Space`.**
- `RecentCommands` — open the frecency-ranked "recent commands" palette
  ([09](09-roadmap-risks.md) §3.2). **Unbound by default** (the natural `Ctrl+R`
  analog is left free to avoid clashing with the shell's own reverse-search).
