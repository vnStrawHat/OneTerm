# US-0061 GUI walk (Windows 11, `fast-dev` build, Zed One Dark)

Driven with posted mouse messages (`WM_MBUTTONDOWN` / `WM_MBUTTONUP` at the prompt) and
captured with `PrintWindow`; the clipboard was set with `Set-Clipboard` before each click. The
setting-off run replaced `target/terminal.json` (debug config directory) with a copy whose
`mouse.middle_click_paste` is `false`; the file was restored afterwards and both launched
instances were stopped.

| Screenshot | What it shows |
|---|---|
| `US-0061-middle-click-paste-dark.png` | Default settings: one middle click at the `cmd` prompt pasted `echo pasted-by-middle-click` from the clipboard. |
| `US-0061-middle-click-off-dark.png` | `terminal.json` with `"middle_click_paste": false`: the same click with `echo should-not-paste` on the clipboard left the prompt empty. |

Not walked: the Settings › Terminal › Mouse switch itself (a separate GPUI window the posted
key path cannot open without modifiers); it is built with the same `SettingField::switch` as
"Copy on Select" and its persistence is covered by the `persist_tests` round-trip. A program in
mouse mode (unit-tested with the fake session) was not exercised against a real TUI.
