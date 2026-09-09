# High-Level Design: One SSH page carrying the SFTP options

Intake: IN-0021
Lane: normal
Date: 2026-09-09

## Idea

`crates/settings-ui/src/ssh.rs` becomes the only page for remote-session settings: it keeps
the "Connection" group and gains the two groups that `sftp.rs` used to own, renamed
"SFTP Editor" and "SFTP Edit Limit". `sftp.rs` is deleted and `panel.rs` drops its page from
`pages()`. Nothing else moves: the same setting getters/setters still read and write the `ssh`
and `sftp` groups of `TerminalSettings` through `crate::terminal::set`, so `terminal.json`
keeps its shape and page-level "Reset All" still walks every field via `default_value`.

## Diagram

```text
panel.rs pages()
  general · key_bindings · terminal · ssh(cx) · appearance · about
                                        │
                                        ▼
                              ssh.rs  SettingPage "SSH"  (IconName::Network, resettable)
                                        ├── Connection        → TerminalSettings.ssh
                                        ├── SFTP Editor       → TerminalSettings.sftp.editor
                                        └── SFTP Edit Limit   → TerminalSettings.sftp.edit_max_file_size
```

## UI Wireframe

```text
+---------------------------------------------------------------------+
| Settings                                                    [_][x]  |
+---------------------------------------------------------------------+
| General        |  SSH                                  [ Reset All ] |
| Key Bindings   |                                                     |
| Terminal       |  +-- Connection ---------------------------------+  |
| > SSH          |  | Keepalive settings applied to newly opened    |  |
| Appearance     |  | SSH sessions.                                 |  |
| About          |  |  Enable Keepalive               [x]           |  |
|                |  |  Keepalive Interval (seconds)   [   30  ]     |  |
|                |  |  Keepalive Max                  [    3  ]     |  |
|                |  +-----------------------------------------------+  |
|                |                                                     |
|                |  +-- SFTP Editor --------------------------------+  |
|                |  | Which editor the SFTP browser's Edit action   |  |
|                |  | opens a remote file with.                     |  |
|                |  |  Editor            [ OS default application v]|  |
|                |  |  Custom Program    [                        ] |  |
|                |  |  Custom Arguments  [                        ] |  |
|                |  +-----------------------------------------------+  |
|                |                                                     |
|                |  +-- SFTP Edit Limit ----------------------------+  |
|                |  | Limits for opening remote files for editing.  |  |
|                |  |  Max Edit File Size (MB)        [    1  ]     |  |
|                |  +-----------------------------------------------+  |
+---------------------------------------------------------------------+
```

The sidebar has no "SFTP" entry; the "Custom Program" / "Custom Arguments" rows stay disabled
until the Editor dropdown is "Custom command", exactly as on the old SFTP page.

## Data Flow

1. `SettingsPanel::render` rebuilds the pages every frame and calls `ssh::page(cx)`.
2. `page` composes the three groups; the SFTP editor group reads the current editor mode from
   `TerminalSettings::global(cx)` to decide whether the custom rows are disabled.
3. A field setter calls `crate::terminal::set`, which mutates the live `TerminalSettings` and
   persists `terminal.json`; the panel observes the entity and re-renders.
4. "Reset All" walks the page's groups and calls each field's `default_value` setter — the
   same values as before, now reached from one page.

## Detail Design

- [ ] Detail design: not needed
- Reason: one file move inside a single crate, no new contract or data flow.
