# Work: Focus the SSH private-key passphrase input

ID: BUG-005
Created: 2026-08-11

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Keep operational status in `harness.db`.

## Classification

- Change type: bug
- Risk lane: normal; the affected value is secret-sensitive, but the change does not alter storage or backend authentication.
- Spec Intake, when required: existing capability `IN-0004`; no new intake required.

## Outcome

After switching or opening a connect dialog with Private Key authentication, the visible Passphrase input can receive pointer focus and keyboard input in both Quick Connect and saved-session dialogs.

## Scope

- In scope: connect-time authentication form focus behavior when Private Key is selected, including focus restoration after native key selection and the saved-session dialog's initial-focus lifecycle.
- Out of scope: persistence, backend key loading, credential lifetime, dialog redesign, and SSH-agent support.

## Acceptance

- Password and Private Key conditional branches have distinct stable GPUI element identities, so stale input hitboxes/focus state are not reused when the form switches.
- Selecting Private Key defers passphrase focus until after the refreshed form has been laid out.
- Returning from Browse with a selected key defers focus to the passphrase input, allowing immediate entry.
- Saved-session initial focus runs once and is not reapplied by recurring dialog renders.
- Password and key-path inputs remain interactive.
- Passphrase zeroization, non-persistence, and empty-passphrase behavior are unchanged.

## Documentation

### Owning Docs Reviewed

- `docs/ssh-authentication.md` — requires an optional masked passphrase field for private-key flows.
- `docs/work/US-004-connect-with-an-ssh-private-key.md` — original acceptance and known manual E2E gap.
- `Screenshot 2026-08-11 132116.png` — runtime evidence that Private Key retained the blue focus border while the visible Passphrase field could not receive input.
- `reference/gpui-component/crates/ui/src/input/input.rs` — pinned Input identity and focus tracking behavior.
- `vendor/gpui-component/src/dialog/dialog.rs` — current dialog content and focus-trap rendering lifecycle.

### Documentation Action

No contract change: `docs/ssh-authentication.md` already states the intended editable passphrase behavior. This packet records the implementation defect and proof.

Reason: the accepted user behavior does not change.

### Reconciliation

No owning-contract update was required; `docs/ssh-authentication.md` already specifies an editable optional passphrase field.

## Context

The passphrase `InputState` is created correctly and rendered with a unique entity-backed Input ID. Runtime isolation established that Quick Connect works and only the saved-session dialog opened from a session item fails. The saved-session `open_dialog` builder called `focus_handle.focus(...)` unconditionally. `gpui-component::Root` invokes a stored dialog builder on every render, not only when the dialog opens. Clicking Passphrase changes focus and triggers another render; the builder then immediately forces focus back to the Private Key path. The fix guards initial focus with a one-shot `Cell` and defers that one focus operation until after initial layout. A code-style review removed the earlier foreground hit-target workaround because normal `Input` pointer focus already works and the workaround was unrelated to the saved-dialog root cause.

## Plan

1. Keep stable, distinct Password and Private Key subtree IDs and normal `Input` pointer behavior.
2. Make the saved-session dialog's initial focus one-shot instead of reapplying it from its recurring builder on every render.
3. Defer the single initial focus operation until after initial layout.
4. Cover both pointer-plus-keyboard input and non-reapplied initial focus with GPUI regression tests, then run focused/workspace gates.

## Decisions

No new durable decision.

## Verification Plan

- GPUI visual regression test: render `SshAuthForm` in a `Root`, click the passphrase text area, assert focus, type text, and assert its input value.
- Focused `oneterm-session-ui` tests and clippy.
- Workspace formatting, clippy, build, and tests.
- Manual confirmation in the full production dialog remains useful, but pointer focus and typing are now mechanically covered at the shared form level.

## Evidence and Gaps

- `cargo test -p oneterm-session-ui passphrase_input_accepts_pointer_focus_and_keyboard_input -- --nocapture` — passed; the GPUI visual test clicks the rendered Passphrase input, asserts its `FocusHandle`, simulates keyboard text, and asserts the passphrase value.
- `cargo test -p oneterm-session-ui initial_dialog_focus_is_not_reapplied_after_focus_moves -- --nocapture` — passed; the regression test performs initial focus, moves focus, reruns the one-shot helper as a recurring builder would, and confirms the user's focus remains unchanged.
- `cargo clippy -p oneterm-session-ui --all-targets -- -D warnings` — passed.
- `cargo fmt --all -- --check` — passed.
- `cargo clippy --workspace --all-targets -- -D warnings` — passed.
- `cargo build --workspace` — passed.
- `cargo test -p oneterm-session-ui` — passed: 12 tests.
- `cargo test --workspace` — passed: 572 passed, 2 ignored, 68 filtered out.
- Code-style review removed the obsolete foreground focus overlay, restored normal gpui-component Input behavior, reduced the shared render API, corrected changed import groups and stale comments, and replaced abbreviated auth clone names with purpose-specific names.
- User feedback and `Screenshot 2026-08-11 132116.png` disproved the earlier shared-form-only fixes. Follow-up runtime isolation showed Quick Connect works while the saved-session dialog does not, identifying its unconditional recurring focus assignment as the remaining root cause.
- The shared form has executable pointer-plus-keyboard proof, and the saved-session one-shot focus lifecycle has a focused regression test. Final confirmation in the complete production dialog remains pending.

## Handoff

Not applicable.
