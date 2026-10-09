# US-0157 independent verification

Packet: [`US-0157`](../US-0157-vault-lifecycle-ui.md) (high-risk lane: secrets in memory, a UI
that writes and deletes `vault.json`)
Verified head: `423a1bf7` (implementer branch `worktree-agent-acca1ea8d0a12015d`, base `main` @
`edf77c55`)
Verifier branch: `verify/US-0157`, in the implementer's worktree (disk at about 16 GB free)
Date: 2026-10-09, Windows 11, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree-local `target`

## Verdict: PASS, with two Medium defects to fix before US-0158 / US-0159 build on `VaultState`

Every acceptance item checked here holds: the gate is green, Argon2id never runs on the UI
thread, Locked never re-reads `vault.json`, the back-off goes 2, 4, 8, 16, 30 s and refuses even
the right password while it runs, the rehash skips the three refusals, the elevated state refuses
every write, the page and the crate agree that a newer file cannot be reset, and the GUI walk
repeats the implementer's results with the files inspected. No defect is exploitable in this
packet alone, because no sealed value exists before US-0158. Two defects in the state machine
(D1, D2) become real as soon as US-0158 and US-0159 call it. Each is a few lines to fix. The
cheapest course is a short rework of this packet before merge. Otherwise they must be carried
forward as conditions on US-0158 (D1) and US-0159 (D2).

## Defects

| # | Severity | Where | What |
| --- | --- | --- | --- |
| D1 | Medium (latent, blocks US-0158 callers) | `crates/state/src/vault_state.rs:277-284`, `302-311`, `331-338`, `359-370` (every operation) | `busy` is cleared only by the `cx.spawn` step that the returned `Task` holds. Dropping that `Task` before it completes cancels the step, so `busy` stays `true` for the rest of the process. Every later set-up, unlock, change and reset answers `Busy` until restart. The module doc says "await it or detach it", but nothing enforces that. `let _ = state.unlock(..)` or a task stored in a view that closes first is enough. Today's only caller (`vault_dialogs.rs:312-317`) detaches, so this packet is not affected. |
| D2 | Medium (security-relevant once US-0159 lands) | `crates/state/src/vault_state.rs:471-487` (`finish_change`) | A Lock while a change runs is undone when the change finishes. `lock()` is allowed during `busy` (by design), but `finish_change` puts the new `UnlockedVault` back and emits `Unlocked` even though the phase is now Locked. The key returns to memory with no password typed after the lock. It is reachable from the GUI today: Cancel the Change dialog (it closes while Argon2id runs), then click Lock Now. US-0159's idle lock goes through the same path. `finish_rehash` already handles this case correctly (`:527-531`). |
| D3 | Low (memory hygiene, documented limit understated) | `crates/state/src/vault_dialogs.rs:277-281`, `:416` | gpui-kit's `InputState::value()` builds a fresh `SharedString` from the rope on every call (`reference/gpui-kit/crates/base/src/input/base/state.rs:1242`). `submit` collects one such copy per field, and `render` calls it for the new-password field to compute the strength hint, which leaves one copy of the new master password per frame. None of these copies is zeroized. The module doc's "copied into a `SecretString` once at submit" is therefore optimistic. Fix: count characters with `input.read(cx).text().len_chars()` (no allocation), and build each `SecretString` straight from `text().to_string()`. |
| D4 | Low (documentation) | `low-level-design/persistence-and-migration.md` § 8 (and the `unlock` comment in `low-level-design/idle-lock-overlay.md` § Interfaces) | Both still say that a dialog unlock re-reads `vault.json`. Plan choice 1 ("one unlock path") replaced that, but only `high-level-design.md` and the packet were updated. US-0158 and US-0159 will read the LLDs. The idle-lock LLD § 5 also states the formula `min(2^(n-5), 30)`, which does not produce the sequence it lists. The code follows the list. |

Not defects, recorded for the next packets (see § Deviations): a reset whose removal fails after
`Reset` was emitted (attack e); `ensure_unlocked` in an elevated window with no vault opens a
Set up dialog that is always refused (attack f); `reset()` from Unlocked emits only `Reset`,
never `Locked`.

### Reproductions (tests added to `crates/state/src/vault_state_tests.rs`, all passing)

The tests named `verifier_d<N>_...` pin today's behaviour. Invert them when the defect is fixed.

- D1 `verifier_d1_a_dropped_operation_task_leaves_the_state_busy`: unlock, `drop(task)`,
  `run_until_parked()` gives `is_busy() == true`, phase Locked, and the next unlock is `Busy`.
- D2 `verifier_d2_a_lock_during_a_change_is_undone`: from Unlocked, start a change, `lock()`
  (phase Locked), await the change. The phase is Unlocked and the events are `[Locked, Unlocked]`.
- Also added (behaviour correct): `verifier_lock_during_a_rehash_keeps_the_new_header`,
  `verifier_set_up_after_another_process_recovers_to_locked`,
  `verifier_a_second_set_up_is_busy_then_wrong_phase`,
  `verifier_change_and_unlock_share_the_failure_count`,
  `verifier_a_failed_reset_has_already_announced_itself` (Windows only), and a compile-time
  assertion that `VaultState` is neither `Debug` nor `Clone`.

### Proposed fixes (not applied)

- D1: keep the apply step out of the caller's hands. Spawn it and `detach()` it inside
  `VaultState`, and give the caller a `Task` that awaits a `futures::channel::oneshot` receiver
  for the result (`futures` is already in `Cargo.lock` through gpui), or a receiver of any
  equivalent kind. Dropping the caller's task then loses only the answer, never the `busy` reset.
- D2: record whether the operation started Unlocked. In `finish_change`, if it did and the phase
  is now Locked, keep `Phase::Locked(vault.header().clone())` (mirroring `finish_rehash`) and
  emit nothing. US-0159 must apply the same rule to `IdleLocked` in `finish_change`,
  `finish_set_up` and `finish_rehash`.

## Gate (command log)

| Command | Result |
| --- | --- |
| `git status` (worktree, before) | clean; `git checkout -B verify/US-0157 423a1bf7` |
| `cargo fmt --all -- --check` | pass (also after the verifier tests) |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass (also after the verifier tests) |
| `cargo test -p oneterm-state -p oneterm-settings-ui -p oneterm-vault -p oneterm-actions` | pass: state 59, settings-ui 58 + 1 ignored, vault 50, actions 4 |
| `cargo test -p oneterm-state` (with the verifier tests) | pass: 66 (59 + 7 new) |
| `cargo test -p oneterm-settings-ui --lib -- --ignored --test-threads=1` | pass: 1 (`updates::config::an_elevated_window_queues_no_update_config_write`, which already existed) |
| `python scripts/verify-dependency-graph.py` | pass (21 packages) |
| `python scripts/check-doc-paths.py` | pass (223 paths) |
| `python scripts/check-english.py` | pass (1112 files) |
| `python scripts/third-party-notices.py --check` | pass |
| `python scripts/check-ignored-tests.py` | pass, 19 ignored tests, all recorded. **US-0157 adds no ignored test**, so the census needs no new row. The elevated cases are plain unit tests that pass `read_only = true` to `VaultState::load` instead of flipping the process global. |
| `git diff main..HEAD -- Cargo.lock` | `+ "log"` and `+ "oneterm-vault"` in `oneterm-state`'s dependency list only; **no new package** |
| `%TEMP%` `oneterm*` entries before / after all runs | 42 / 42 (all from older crash-report and dock tests); no `oneterm-vault-state-*` or `oneterm-security-page-*` left |
| `cargo build -p oneterm-app` (in place, for the GUI walk) | pass; relinked `target/debug/oneterm.exe` (only the test file had changed) |

`oneterm-vault`'s `test-support` feature is enabled only as a dev-dependency of
`oneterm-state`. The workspace uses resolver 3, so the feature never reaches a normal build.
Its two functions keep the elevated guard (`is_restricted()`), as production `init` does.

## Attacks

### (a) `busy` lifecycle

- **Dropped task:** D1, confirmed by test. Severity: Medium. The fix belongs in `VaultState`,
  not in every caller (one guard where all callers route through).
- **Lock during a rehash:** correct. Lock is allowed, and set-up / unlock / change / reset
  answer `Busy` while the rehash runs. After it, the phase stays Locked on the rewritten header,
  so unlock works and a later change is not refused as "changed elsewhere" (test).
- **Lock during a change:** D2.
- **Change / Reset during a rehash:** `Busy` (test). The page also disables Set Up, Unlock,
  Change and Reset while busy (`security.rs:99,106,124,157`); Lock Now stays enabled.
- **Cancel during set-up, then reopen:** the set-up still completes (the dialog detaches the
  awaiting task, `vault_dialogs.rs:312-317`). A second set-up while the first runs is `Busy`.
  One after it is `WrongPhase`, and only the first password unlocks (test).
- **`AlreadyInitialised` while Uninitialised:** `finish_set_up` re-reads the file and moves to
  Locked on the other process's header, and that password unlocks (test). This is the only
  re-read after start, and it only ever leaves Uninitialised.

### (b) Secret handling in `vault_dialogs.rs::submit`

The typed text goes from the input's `Rope` to a `SharedString` (one per field, `:277-281`), then
`to_string()` to a `String`, which moves into a `SecretString` (zeroized on drop). For an unlock,
the `SecretString` moves into the background closure. On failure it drops there. On success it
returns with the vault and moves into the rehash closure, which drops it when the rewrap ends.
For set-up it moves into `create_vault`'s closure. For change, `old` and `new` move into
`rewrap`'s closure. No copy is kept in the form, in `on_ready` or in any closure that outlives
the operation. The `SharedString`s and the input's rope are not zeroized (documented limit), and
D3 adds more copies than documented. A rejected password is cleared with `set_value("")`, which
also clears the undo history (`state.rs:933-952`). Only the first field is cleared, which is the
right one for both Unlock and Change. No `{:?}` touches a secret: `VaultActionError` is `Debug`
but carries only `VaultError` (whose variants hold no secret material, `error.rs:1-2`). Logging
covers `VaultError` `Display` and the damaged reason only (`vault_state.rs:533,535,546,557`).
`VaultState` is neither `Debug` nor `Clone`, which is now asserted at compile time.
`SecretString`'s `Debug` prints `***`. The GUI walk's logs contain neither typed password.

### (c) Back-off

`backoff_after(5..=10)` is `[2, 4, 8, 16, 30, 30]` and saturates at `u32::MAX` (existing test).
The countdown cannot run twice (the `ticking` flag is set before the spawn and cleared only by the
loop's last tick, `:355-377`). It cannot stall at 0: `retry_after()` filters a zero wait, the loop
ends on the first tick with nothing to wait, and the message rounds up, so "0 s" never shows. A
right password during the back-off is refused in two places: `can_submit` in the dialog, and
`check_attempt` in `VaultState` (test). In the GUI, the right password typed during the 4 s
back-off and submitted with Enter stayed in the field and was not tried. A success resets the
count (existing test). **Counting a wrong old password in Change is not just acceptable but
required.** Change from Locked with the right old password unlocks, so an uncounted Change would
bypass the back-off. The verifier test proves the two share one count, and that a right old
password is refused (`RetryIn`) during the back-off.

### (d) Locked never re-reads `vault.json`; the Unavailable phase

`oneterm_vault::load` has one call site, `read_phase` (`vault_state.rs:542`). `read_phase` is
called from `load` (start) and from `finish_set_up` on `AlreadyInitialised` (Uninitialised only).
Unlock uses the header held in memory (`:295-305`), as the existing tests show (file deleted, and
file replaced by a passer-by's vault). Unavailable comes in three kinds. Damaged gets Reset; the
crate resets Ready and Damaged. Newer (`Unsupported`) gets no Reset, and the crate refuses it.
Unreadable gets no Reset; `oneterm_vault::reset` would fail too, because `write(Remove)` loads
first. The UI and the crate agree in every case. In the GUI, a damaged file showed the reason and
only Reset; a `schema_version 9` file showed "created by a newer OneTerm", no button, and was left
byte-identical.

### (e) Reset ordering (plan choice "Reset order")

`Reset` is emitted before the removal, as `oneterm_vault::reset`'s own contract demands ("Every
owner of sealed values must wipe them **before** this runs", `vault.rs`; `crypto-and-format.md`
§ 8). A removal that then fails was forced in a test by holding `vault.json` open without
sharing. The owners have wiped, the key is dropped, the phase is Locked, `vault.json` is still
there, and a retry completes the reset. Judged against `DEC-0021` rule 2 ("Reset removes
`vault.json` and every sealed value"): the user confirmed losing every sealed value, nothing
secret is exposed, and the half-done state is recoverable with the same button. Emitting after
the removal would instead leave envelopes behind on a crash. Those are harmless (they read as
`OtherVault`), but they break the crate's documented contract. **Ordering accepted; no fix
proposed.** US-0158 must make its wipe idempotent and must not assume Uninitialised follows
`Reset`. A rarer edge: `remove_document` removes `vault.json` and then `vault.bak`. If only the
second removal fails, the state stays Locked with no file until restart. That is recoverable by
Reset or a restart (informational).

### (f) Elevated instance (Q5)

`VaultState::init` passes `elevation::is_restricted()`, which `app::run` sets
(`crates/app/src/lib.rs:56`) before `init::init` (`:210`). In read-only mode the state refuses
set-up, change and reset with `ReadOnly`, skips the rehash, and allows unlock and lock
(`the_elevated_window_unlocks_but_never_writes`). The page shows the note and only Unlock or Lock
Now, never Set Up, Change or Reset, including for a damaged file
(`the_elevated_page_is_read_only_but_unlocks`). Independently of `VaultState`, every
`oneterm-vault` write checks `is_restricted()`, and so does `remove_document`. The guard is two
deep. **Not walked in a GUI** (UAC elevation needed); the packet's Elevated acceptance item stays
open, for the owner. `ensure_unlocked` in an elevated window with no vault opens a Set up dialog
that every submit refuses. US-0158 never offers saving there (M1), but it should check
`is_read_only()` first.

### (g) UI and code style

Colours come only from `cx.theme()` (`muted_foreground`, `danger`, `warning`). Every string is
English and the paths are relative. Every public item has rustdoc (`VaultState`, `VaultPhase`,
`VaultEvent`, `VaultActionError::message`, `retry_message`, `MIN_PASSWORD_CHARS`,
`RESET_CONFIRMATION`, the dialog openers, `ensure_unlocked`). Enter submits through
`FormDialog`'s `on_ok`, which the GUI walk confirmed for set-up, unlock, change and reset.
Escape / Cancel close the dialog. Argon2id call sites: `oneterm_vault::init` / `init_with_params`
(`create_vault`), `oneterm_vault::unlock` (`unlock`, `rewrap`) and
`change_password[_with_params]` (`rewrap`). All are reached only from `background_spawn`
closures (`vault_state.rs:279,303,333,511`). The UI thread only reads and parses the small
`vault.json` (at start and on `AlreadyInitialised`).

### (h) Rehash

The typed `SecretString` moves into the rehash closure and lives until the rewrap returns
(`:505-517`). `ReadOnly` is never attempted. `VaultReplaced` and `ChangedElsewhere` are logged
at info and leave the file alone (existing tests). On success the second `UnlockedVault` (same
data key) replaces the state's vault, or only its header when the process was locked meanwhile,
so the held header is the rewritten one (existing test, plus the verifier's lock-during-rehash
test).

## GUI walk (debug `oneterm.exe` from this worktree)

Private profile: `USERPROFILE` and the working directory in `scratchpad/us0157v-walk`, so
`config_dir()` is `<walk>/cwd/target`. The driver is `scratchpad/us0157v-gui.ps1`, a copy of the
implementer's, run under Windows PowerShell 5.1, which posts window messages to the launched pid
only. Screenshots: `scratchpad/us0157v-*.png`. Every launched pid (19132, 27180, 18968, 25572)
was stopped by that pid.

| Step | Result |
| --- | --- |
| Set up (`01`-`03`) | The first Enter, with the confirmation typed into the wrong field, showed "The two passwords differ." inline. A correct set-up through **Enter** gave "Set, unlocked." with Change / Lock Now / Reset. `vault.json`: schema 1, argon2id v19, 65536 KiB / 3 / 1, 16-byte salt, `otv1:` key. `vault.bak` was byte-equal, and the password text was in neither file. |
| Restart (`04`, `05`) | "Set, locked." with Unlock, Change and Reset. The Unlock dialog had its input focused. |
| 6 wrong passwords (`11`, `13`) | After the 5th: "Try again in 1 s" (2 s back-off) with Unlock disabled. After the 6th: "Wrong master password." and "Try again in 4 s.", input cleared. |
| Right password during the 4 s back-off (`14`) | Typed and submitted with Enter: not tried, text kept, Unlock disabled. Submitted again after the wait (`16`): "Set, unlocked." |
| Change (`17`-`19`) | Current, then new twice, with the strength hint. Afterwards: same `vault_id`, new salt, old wrapped key in neither file, `vault.bak` byte-equal to `vault.json`, no plain text. |
| Reset (`20`, `21`) | Typing `RESET` and pressing Enter removed both `vault.json` and `vault.bak`. The page went back to "Not set up." with Set Up only. |
| Damaged `vault.json` (`22`) | "vault.json is damaged (not JSON). Reset is the only way back." with Reset only. Log: one `WARN ... left in place`. |
| Newer `vault.json` (`23`) | "vault.json was created by a newer OneTerm (schema version 9 ...)." with no button. The file was byte-identical afterwards. |
| Logs | Neither password appears. Besides the damaged-file warning, only the existing DirectX shutdown lines. |

An early attempt at "right password during the back-off" came too late: the 2 s wait had already
ended while the driver typed, so that password was rightly accepted. The 4 s case above is the
valid one.

## Deviations (the packet's "Choices the LLDs leave open")

1. **One unlock path (no re-read).** Stricter than `persistence-and-migration.md` § 8 and safe:
   a passer-by's file can neither unlock nor block (tests). The cost (a change made in another
   process is seen only after a restart) is documented. Accepted, but D4: the two LLDs still say
   otherwise.
2. **Change and rehash on a second `UnlockedVault`.** Correct: same data key, and the state's
   vault is replaced only on success. The extra Argon2id run is acceptable. D2 is the one gap:
   `finish_change` ignores a lock that happened meanwhile.
3. **The failure count includes a wrong old password in Change.** Required, not just acceptable
   (see attack c).
4. **Reset order.** Accepted (attack e). **US-0158:** wipe on `Reset`, idempotently. The reset
   can still fail afterwards and leave the state Locked.
5. **No new action.** Fine. `LockOneTerm` is US-0159's.
6. **`ensure_unlocked` calls `on_ready` on success only.** It does nothing when the vault is
   Unavailable. **US-0158:** show the reason yourself, check `is_read_only()` before offering to
   save, and remember that `on_ready` never runs on cancel.
7. **Back-off `2^(n-4)` from the 5th failure.** Matches the intake and the listed sequence; the
   LLD's formula is the inconsistent part (D4).
8. **Cancel while busy.** The operation completes anyway. That is acceptable, since the user
   already confirmed. **US-0158/US-0159:** a cancelled dialog does not mean nothing changed;
   re-read the phase.
9. **A read error is Unavailable with no Reset.** Consistent with LLD § 3 ("treated as
   Unsupported").
10. **Spinner through the icon slot.** Fine; the implementer's walk shows it (`us0157-11a`).
11. Also fine: `VaultState::load(dir, read_only)` made public; `AppIcon::Lock`; set-up text that
    does not mention the screen lock yet.

**Must know before building on this packet.** US-0158: D1 (fix it, or every caller must detach);
items 4, 6 and 8 above; the re-read of `vault.json` before each seal (`persistence-and-migration.md`
§ 8) is still owed; `reset()` from Unlocked emits only `Reset`. US-0159: D2 (and the same rule for
`IdleLocked` in every `finish_*`); `lock()` acts only in Unlocked; D4's LLD corrections.

## Gaps

- Elevated window not walked (UAC); covered by unit tests only.
- Linux / macOS not run (CI). The full `cargo test --workspace` was not run here (disk); the
  census built every test target and all of them listed.
- No frame-time measurement during Argon2id. In this debug build the default parameters finished
  in under 0.3 s, too fast to catch the spinner. The implementer's capture `us0157-11a` shows it.

## Re-verification of 6db052a4

Rework head `6db052a4` (built on `8033a6f0`), checked out as `verify/US-0157` in the same
worktree, starting from a clean status. Same settings: `CARGO_INCREMENTAL=0`,
`CARGO_BUILD_JOBS=2`, about 14 GB free.

### Verdict: PASS

D1, D2, D3 and D4 are fixed, and each fix holds under the attacks below. Two Low items remain
(R1, R2). Neither blocks the merge.

| # | Severity | Where | What |
| --- | --- | --- | --- |
| R1 | Low (docs) | root `Cargo.toml:102` comment; `docs/agents/dependencies.md` § allowed crates, "Build / development" row | Both still describe `futures` as test-only / dev-only, but `oneterm-state` now uses it as a normal dependency (`futures::channel::oneshot`). There is no new package: `gpui-pre` already depends on `futures`, and `third-party-notices.py --check` passes. Fix: move it to the async row ("`futures` (`oneterm-state`: `channel::oneshot` for `VaultState` answers; tests: the SSH agent server)") and update the comment. |
| R2 | Low (for US-0159) | `crates/state/src/vault_dialogs.rs:356` | After `LockedMeanwhile` the Unlock dialog stays open with the **right** password still in its input (only `WrongPassword` clears it), so pressing Enter again unlocks without retyping. US-0157's UI cannot reach this: Lock Now is shown only while Unlocked, and the dialog is modal. US-0159's idle lock and shortcut can. Fix there: clear every input on `LockedMeanwhile`, or close the dialog. |

### Attacks

**D1, `run` (`vault_state.rs:419`).** The apply step is spawned and detached inside the state.
The caller's task only awaits a `oneshot` receiver; a sender dropped without a send reads as
`WrongPhase`.
- **What clears `busy` on every exit path:** `begin` (`:442`) sets it. Every `finish_*` calls
  `end` (`:448`) as its first statement (`:473, :505, :531, :548-553, :584`), before any `?`
  or early return. `finish_unlock`'s `WrongPhase` return comes after `end`, and so do
  `finish_change`'s and `finish_reset`'s `result?`.
- **When the apply step never runs:**
  - The entity is gone: `update` fails; there is no state left to be busy, and the receiver
    gets `WrongPhase`.
  - The background work panics: the app is on its crash path anyway (`panic = "unwind"`,
    background thread). Argon2id and AES-GCM do not panic on these inputs.
  - A panic inside `apply` runs on the UI thread, which is the same crash path.
- **Tests:**
  - `verifier_d1_a_dropped_operation_task_still_applies` (inverted by the implementer).
  - New `verifier_r_dropped_tasks_in_a_row_all_apply`: a dropped set-up, then a dropped
    unlock. Both apply, `busy` is clear after each, and the events are exactly
    `[Initialised, Locked, Unlocked]`.
  - Two callers in a row while one runs: the second is `Busy` (existing
    `verifier_a_second_set_up_is_busy_then_wrong_phase`).
- **Dependency:** `futures` is an existing workspace dependency, and `Cargo.lock` gains no
  package. `verify-dependency-graph.py` passes: its policy restricts only workspace edges and
  `argon2` / `aes-gcm`. The stale docs are R1.

**D2, "the lock wins" (`lock` `:351`, flag reset in `begin` `:444`, taken in `end` `:451`).**
- **Lock during a change** started Unlocked: the phase stays Locked on the new header, and only
  `Locked` is emitted (`verifier_d2_a_lock_during_a_change_wins`, inverted).
- **Lock during a change** started Locked: the file changes, the process stays Locked, no
  event is emitted, and only the new password unlocks (new
  `verifier_r_a_change_from_locked_locked_meanwhile_stays_locked`).
- **Lock during a rehash:** the process stays Locked on the rewritten header, then unlocks, and
  a later change is accepted (`verifier_lock_during_a_rehash_keeps_the_new_header`). The
  rehash keeps the password, so the "new" credential here is the new header.
- **Lock during an unlock:**
  - Right password: `LockedMeanwhile`. No key is kept (`unlocked()` is `None`), `busy` is
    clear, the failure count resets (the password was right), there are no events and no
    rehash (`a_lock_during_an_unlock_wins`).
  - Wrong password: still `WrongPassword`, and the count rises to 1 (new
    `verifier_r_a_wrong_password_locked_meanwhile_still_counts`).
  - The key and the typed `SecretString` drop (zeroized) at the `LockedMeanwhile` return.
- **Lock during a set-up:** events exactly `[Initialised, Locked]`, phase Locked, and the
  password unlocks afterwards (`a_lock_during_a_set_up_wins`).
- **An idle lock**, in Unlocked or in Locked, sets no flag, and the next unlock succeeds (new
  `verifier_r_an_idle_lock_does_not_carry_over`).
- **Back to back:** an unlock answered `LockedMeanwhile`, then a plain unlock, which succeeds
  (new `verifier_r_the_operation_after_a_locked_one_is_not_refused`).

**D3 (`vault_dialogs.rs`).** `submit` makes one `SecretString::new(input.text().to_string())`
per field (`:290-297`) and moves each one out with `swap_remove`. The Change order is correct:
removing index 1 then index 0 yields new and old, and the leftover confirmation drops zeroized.
Validation and the `RESET` check go through `expose_secret()`. The strength hint uses
`text().len_chars()` (`:437`) and copies nothing. A grep of the file finds no `value()`,
`unmask_value()` or `selected_value()` call. For passwords shorter than ropey's chunk size, the
rope's `to_string()` writes one chunk, so a single allocation, with no growth reallocations left
behind. The module doc now states the limit accurately.

**D4 (docs).** These now match the code:
- `persistence-and-migration.md` § 8: no unlock re-reads, the cost is stated, and the one
  re-read is on `AlreadyInitialised`.
- `idle-lock-overlay.md` § 5: `min(2^(n-4), 30)`, which gives 2, 4, 8, 16, 30 for n = 5..9.
- `idle-lock-overlay.md` § Interfaces: `unlock` uses the in-memory header only.
- The HLD's as-built note: the lock wins, and a dropped task never leaves the state busy.

**Elevated `ensure_unlocked`.** With no vault (`vault_dialogs.rs:89`), it pushes a Warning
notification: "No master password is set up. Managed from the normal OneTerm window." No Set up
dialog opens. `security.rs:23` reuses `vault_dialogs::READ_ONLY_NOTE`, so the two cannot drift.
This was checked by inspection only: a unit test would need a `Root` window with notifications.
The call is a `WindowExt` call, like `open_dialog` on the other branches, so callers have the
same "not inside a `Root` update" constraint as before (US-0149 lesson).

### Gate (command log)

| Command | Result |
| --- | --- |
| `git status`; `git checkout -B verify/US-0157 6db052a4` | clean; ok |
| `cargo fmt --all -- --check` | pass (also after the new tests) |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass (also after the new tests) |
| `cargo test -p oneterm-state -p oneterm-settings-ui -p oneterm-vault -p oneterm-actions` | pass: state 68, settings-ui 58 + 1 ignored, vault 50, actions 4 |
| `cargo test -p oneterm-state` (with the 5 new `verifier_r_*` tests) | pass: 73 |
| `cargo test -p oneterm-settings-ui --lib -- --ignored --test-threads=1` | pass: 1 |
| `python scripts/verify-dependency-graph.py` | pass |
| `python scripts/check-doc-paths.py` | pass |
| `python scripts/check-english.py` | pass |
| `python scripts/third-party-notices.py --check` | pass |
| `python scripts/check-ignored-tests.py` | pass, 19 ignored tests, all recorded (none added) |
| `git diff main..HEAD -- Cargo.lock` | `oneterm-state` gains `futures`, `log` and `oneterm-vault` as dependencies; **no new package** |
| `%TEMP%` `oneterm*` entries before / after | 42 / 42; no `oneterm-vault-state-*` or `oneterm-security-page-*` |

No GUI walk this round. The rework changes no layout, and its behaviour is covered by the
tests above.
