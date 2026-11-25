# High-Level Design: Improve completion feature

Intake: IN-0008
Lane: tiny
Date: 2026-08-13

## Idea

Treat terminal completion as one controller-owned interaction pipeline:

1. The engine returns ranked `Suggestion { text, replace_from }` values.
2. The controller uses the replacement range to determine whether the bounded result list contains a useful edit. A sole byte-exact match is cleared; prefixes, multiple choices, and case-only corrections remain visible.
3. Visible results begin unselected. Tab explicitly selects row 0. Until then, Enter and navigation keys continue to the shell.
4. Once selected, navigation moves within the result list and Tab/Enter accept.
5. Acceptance compares the same replacement-range text with the selected suggestion. Unix appends only an exact-case remainder. Cmd/PowerShell may erase the case-mismatched tail and write the displayed suggestion's exact suffix.

This unifies visibility, selection, navigation, and application around `Suggestion::replace_from` without changing engine ranking, history ownership, persistence, or settings schema.

## Diagram

```text
Terminal line + cursor
        |
        v
Completion engine ---> suggestions { text, replace_from }
        |
        v
Controller bounded results
        |
        +-- sole byte-exact match? -- yes --> clear / overlay hidden
        |
       no
        v
overlay visible, selected = None
        |
        +-- Enter / Up / Down / Ctrl aliases --> shell
        +-- Tab (accept_tab on) --------------> selected = Some(0)
                                                    |
                             Up/Down/Ctrl aliases --+--> move selection
                                                    |
                                  Tab or Enter -----+--> acceptance
                                                          |
                    +-------------------------------------+------------------+
                    |                                                        |
             Unix exact prefix                                  Cmd/PowerShell CI prefix
                    |                                                        |
             append remainder                         append remainder or Backspace
                                                      mismatch tail + exact suffix
                    |                                                        |
                    +--------------------------- write bytes to PTY ----------+
                                                     |
                                               dismiss overlay
```

## Data Flow

1. The terminal view extracts the command line at the cursor and requests completion candidates.
2. Whole-line history uses `replace_from = 0`; token candidates use their parsed token start.
3. After bounded truncation, the controller slices `line[replace_from..cursor]`, normalizing leading prompt-space only for whole-line replacement.
4. If exactly one result is byte-identical to that typed slice, the list is cleared. Multiple results, missing suffixes, and case differences remain actionable.
5. Recompute initializes `selected = None`. Dismiss and prompt/gating transitions also reset selection.
6. With no selection, Enter and navigation return to the normal terminal keyboard path. First enabled Tab selects row 0 without writing to the PTY.
7. With a selection, navigation is clamped to the result list and Tab/Enter request acceptance.
8. Acceptance reuses the replacement-range slice. Unix requires exact casing and appends the missing suffix. Cmd/PowerShell accept case-insensitive prefixes; when casing differs, the controller emits one terminal Backspace per character in the mismatched typed tail and then the exact displayed suffix.
9. The view writes acceptance bytes to the PTY and dismisses the overlay.

## Invariants

- Text before `replace_from` is never edited.
- Fuzzy/non-prefix acceptance stays disabled unless explicitly configured.
- A case-only Windows suggestion remains visible because acceptance can make an observable correction.
- Before explicit selection, completion does not capture Enter or navigation history keys.
- Completion acceptance edits input only; it does not execute the command by itself.

## Detail Design

- [x] Detail design: not needed
- Reason: the merged work remains a localized controller/view interaction change with no persistence, authentication, public API, or cross-crate architecture impact.
