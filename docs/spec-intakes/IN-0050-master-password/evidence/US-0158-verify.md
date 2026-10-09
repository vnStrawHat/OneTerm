# US-0158 independent verification

Packet: [`US-0158`](../US-0158-ssh-saved-passwords.md) (high-risk lane: secrets at rest, a
persisted field in `ssh_session.json`, credentials sent to a server)
Verified head: `e5e53104` (implementer branch `feat/us-0158-ssh-saved-passwords`, base `main` @
`0e844d43`)
Verifier branch: `verify/US-0158`, in the implementer's worktree (disk at 13-14 GB free)
Date: 2026-10-09, Windows 11, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree-local `target`

## Verdict: PASS, with four Low defects (none blocks the merge)

Every acceptance item checked here holds. The gate is green. `ssh_session.json` and its `.bak`
only ever hold `otv1:` envelopes. The `.bak` scrub is decided under the lock against the file
being replaced, so it survives the coalescing queue, a deleted session and a Duplicate. The field
needs no schema bump, and the real flattened pre-change entry reads what the current binary
writes. A seal re-reads `vault.json` first. The GUI walk on a fresh private profile repeats the
main path. The walk covered save, Set up, a restart with one unlock, Enter to unlock and Enter to
connect, a two-hop chain with nothing typed, a rejected saved password (row revealed, envelope
kept), an update that reseals and scrubs, and a reset that wipes the file and its `.bak`. No
plaintext was found anywhere in the profile or the logs. The defects below are about attribution
and UX edges, not about secrets leaking.

## Defects

| # | Severity | Where | What |
| --- | --- | --- | --- |
| D1 | Low | `crates/session-ui/src/jump_hops.rs:122-125` (`form_for_error`) | Matching a hop's failure by `message.contains("jump host {user}@{host}:{port}:")` picks the wrong hop when one IPv6 hop's address is another's followed by `:<group>`. Example: hops `ops@2001:db8::1` port 22 and `ops@2001:db8::1:22` port 22. The second hop's failure reads `jump host ops@2001:db8::1:22:22: ...`, which contains the first hop's pattern, and `find` takes the first match. The first hop's row is revealed and its saved password is no longer used. The rejected hop keeps "Using saved password", so the retry fails again. Ports sharing a prefix (22 / 222 / 2222) and a plain `::1` are matched correctly. Fix: the LLD § 6 structured hop index, or bracket IPv6 hosts in the label and match the prefix at the start. Pinned by `d1_an_ipv6_hop_whose_address_extends_another_is_misattributed`. |
| D2 | Low | `crates/session-ui/src/common.rs:517-518` (pre-existing flow) + `connect_dialog.rs:519-524` | The rejected-saved-password handling (LLD § 6) does not happen when the rejection follows a first-use host-key approval. `HostKeyUnknown` closes the connect dialog before the alert opens, and the approved retry carries the same `on_failed`. That callback reveals the row of a dialog that is gone, so the user sees only the error toast. Reopening the dialog shows "Using saved password" again. It shows in practice when a server is rebuilt (new host key and new password). Walked: steps 21-22 below. A second Connect from the reopened dialog then behaves as § 6 (step 23). |
| D3 | Low | `crates/session-ui/src/session_dialog.rs:172` + `auth_form.rs:302-306`, `:364` | In the session dialog, **Replace** followed by unticking the box removes the saved password on Save, silently, and `.bak` is scrubbed. After Replace the box reads "Update saved password", and unticking it reads as "do not update", which in the connect dialog means "keep". `password_plan` cannot tell this state from **Remove** (`keep_saved = false`, `save_typed = false`, so `Store(None)`), and the existing test pins it as "Remove clicked" (`session_dialog.rs:1072`). Fix: have Replace keep a "pending replace" flag so that unticking returns to the saved row, or show the removal note. |
| D4 | Low (latent) | `crates/session-ui/src/session_state.rs:862` with `crates/app/src/init.rs:37,45` | The reset wipe subscribes only `if let Some(vault) = VaultState::try_global(cx)`. Today `VaultState::init` runs before `oneterm_session_ui::init`. If the order is swapped, the subscription is skipped silently, and a reset then leaves every envelope in `ssh_session.json`. Nothing tests or asserts the order, and the `init.rs` comment does not mention it. The envelopes would be unopenable once `vault.json` is gone, so this is not a leak, but the "reset wipes" acceptance would fail with no signal. Fix: `VaultState::global(cx)` (panics like the other startup invariants) or a `debug_assert!`, plus a line in `init.rs`. |

### Not defects (recorded)

- **I1, a stale `.bak` after the document went missing.** `atomic_write_sealed` treats a missing
  document as `Private`, so when `ssh_session.json` was deleted by hand or quarantined
  (`quarantine_file` renames it), the old `ssh_session.bak` is left as it is by the first write,
  envelopes included. The second write replaces it. A reset with nothing in memory writes
  nothing, so it does not scrub that `.bak` either. The quarantined copy also keeps its
  envelopes. After a reset they cannot be opened (the data key is gone with `vault.json` /
  `vault.bak`). Pinned by `a_stale_backup_survives_the_first_write_after_the_document_went_missing`.
- **I2, a window between the re-read and the seal.** `VaultState::seal` (`vault_state.rs:305`)
  re-reads `vault.json`, then seals in memory, and the store writes later on a background
  thread. A reset in another process inside that window leaves an envelope under a dead vault.
  It reads as `OtherVault` at the next Connect (the row asks for the password). This is the § 8
  "last writer wins" limit, not a new one.
- **I3, a transient read error counts as a reset.** `seal` treats any `load` result other than
  `Ready` as a reset. A read error (for example a sharing violation while another process
  replaces `vault.json`) drops the key and shows "reset in another OneTerm window". This needs a
  rename racing the read; the cost is one unlock.
- **I4, `close_dialog` closes the top-most dialog.** The deferred save (`session_dialog.rs:744-746`)
  and the Unlock dialog's own `finish` each close the top of the stack. The walk confirmed that
  this is the session dialog in the normal case (step 8). A dialog pushed on top while Argon2id
  runs would be closed instead. The only asynchronous pusher is a host-key alert from a
  dialog-less retry. In that case the Unlock dialog stays open, and a second Save of a **new**
  session would add it twice. This pattern (`common.rs:478`, `:517`) predates this packet. Clicking
  outside the Set up dialog dismisses it, which counts as a cancel. The session dialog's Save
  underneath cannot be clicked through it (step 6).
- **I5, Quick Connect.** It uses a picked hop's saved password only while the vault is unlocked
  (`quick_connect_dialog.rs:97`). Locked, the row is an ordinary input with no note. A rejected
  saved hop password there is not revealed (`on_failed` only sets the inline error,
  `quick_connect_dialog.rs:402`), so the user must click "Use a different one". This is
  consistent with the As-built note and LLD § 7 (Quick Connect is a follow-up surface).
- **I6, the dev server's `localhost`.** The `direct-tcpip` relay accepts the literal
  `localhost` and resolves it. A hosts file that maps `localhost` elsewhere would relay off the
  machine. This is a dev tool only.

## Command log (gate)

| Command | Result |
| --- | --- |
| `git status` / `git checkout -B verify/US-0158 e5e53104` | clean; branch created |
| `cargo fmt --all -- --check` | pass (head, and again with the verifier's tests) |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass (head, and again with the verifier's tests) |
| `cargo test -p oneterm-session-ui -p oneterm-state -p oneterm-core -p oneterm-ssh -p oneterm-vault -p oneterm-tools` (head) | pass: session-ui 87 + 1 ignored (the existing elevation test), state 75, core 92, ssh 109, vault 50, tools 14 + bins (sftp-dev-server 1, highlight-bench 2, corpus_check 2) |
| `cargo test -p oneterm-session-ui` (with the verifier's tests) | pass: 94 passed (87 + the verifier's 7), 1 ignored |
| `python scripts/verify-dependency-graph.py` | pass (21 packages; the new `session-ui -> vault` edge is in the policy) |
| `python scripts/check-doc-paths.py` | pass (223 paths) |
| `python scripts/check-english.py` | pass (1116 files, with this document and the new test file) |
| `python scripts/third-party-notices.py --check` | pass |
| `python scripts/check-ignored-tests.py` | pass: "19 ignored tests, all recorded" (run here: the disk had 13 GB free and kept 12 GB) |
| `git diff main..HEAD -- Cargo.lock` | one line: `oneterm-vault` added to `oneterm-session-ui`'s dependencies; **no new package** |
| new `#[ignore]` in `git diff main..HEAD` | none (the one hit is the packet's evidence text) |
| `%TEMP%` `oneterm-*` directories before / after the runs | no `oneterm-session-*`, `oneterm-vault-*` or `oneterm-persistence-*` left by the test runs; the three `oneterm-sftp-dev-<pid>` roots of this walk's killed dev servers were removed (six older ones from the implementer's walk remain) |

## Attacks and results

**(a) `atomic_write_sealed` and the scrub decision.** Read `persistence.rs:88-99`, `:118-156`
and `session_state.rs:579-592`. The decision runs under the inter-process lock, against the
bytes of the file being replaced. The rule is "some sealed value there is missing from the new
document". An unparsable previous file counts as holding one.

- Delete of the only session holding an envelope: scrubbed. Delete of one of two Duplicate
  copies: ordinary write, `.bak` keeps the previous generation (the value is still current).
  Test `deleting_a_session_scrubs_only_when_its_envelope_leaves_the_document`.
- Coalesced queue (Replace then rename, only the rename written): scrubbed. Unparsable previous
  file: scrubbed. Swapping two envelopes between sessions: ordinary. Test
  `the_scrub_is_decided_against_the_replaced_file_not_the_last_snapshot`.
- Invariant, argued by induction over writes: a scrub sets `.bak` = current, and an ordinary
  write sets `.bak` = previous, whose sealed values are a subset of the new ones. So `.bak` never
  holds a value the current file does not. I1 is the one way in (a missing document).
- Crash between the temporary write and the rename. In a `Secret` write `.bak` is already the
  new document and the main file is still the old one. The retired envelope survives in the main
  file until the next write. That is the accepted one-operation window of § "Edge Cases"; there
  is never a plaintext.
- Unix `0600`: the temporary file is created `0600` for `Private` and `Secret`, `.bak` gets an
  explicit `set_permissions(0600)` in the `Private` copy, and the `Secret` copy inherits the
  temporary file's mode through `fs::copy`. **Review only, not run** (Windows box; the packet's
  `cfg(unix)` test did not run either).

**(b) Connect dialog.**

- Initial focus is the saved row (`auth_form.rs:207`, `connect_dialog.rs:230-237`), and it is
  reapplied after the unlock (`refocus`, `:299-306`). Walk step 16: the master password, Enter,
  then a second Enter about one second later. The first Enter unlocked, the second connected.
  Exactly one connection reached the jump host (server log: one accepted auth per attempt), so
  there was no double connect and no second Unlock dialog. While the vault form is busy,
  `can_submit` is false, so an Enter during Argon2id is ignored.
- Hop matching: D1.
- A rejected saved password keeps its envelope: the file was byte-identical before and after
  (walk step 23).
- "Update saved password" seals only in `on_connected`. The walk saw the envelope replaced only
  after the server accepted the new password, with the old value gone from both files (step 24).

**(c) Session-dialog deferred save.** `ensure_unlocked`'s `on_ready` runs only after the vault
form has closed itself, so the second `close_dialog` closes the session dialog (step 8: both
gone, session saved with an envelope). Cancel of Set up (and a click outside it, step 6) leaves
the dialog open with the box ticked, and no `vault.json` or `sealed_password` is written. Other
modals: I4.

**(d) Reset wiring.** D4 (init order). The wipe is idempotent:
`clear_sealed_passwords` writes only when something changed (test
`the_reset_wipe_is_idempotent_and_leaves_no_envelope_on_disk`). Walk step 28: after Reset,
`ssh_session.json` and `ssh_session.bak` hold no `sealed_password` and no `otv`, both sessions
are kept, and `vault.json` / `vault.bak` are gone. Stale `.bak` case: I1.

**(e) `VaultState::seal` refusal.** A mismatched or missing `vault.json` drops the key, sets the
phase from the file and emits `Locked` (the state test covers both "replaced" and "removed").
The session dialog shows the warning toast "The master password was reset in another OneTerm
window. Nothing was saved." and stays open. The session (including non-password edits) is not
saved until Save succeeds. The connect dialog says "Connected, but the password was not saved:
...". TOCTOU: I2. Transient read error: I3.

**(f) Old binary.** `deny_unknown_fields` appears nowhere in `crates/session-ui` or
`crates/core/src` (the only hit is the implementer's comment). The implementer's simulation
deserialises the array into a struct with `id` inline. The real entry is
`#[serde(flatten)] session`, a different serde path, so the verifier added the faithful shape
(`OldSession` + `OldEntry` with `flatten`, attributes copied from `main`). Test
`the_old_flattened_entry_reads_what_the_current_binary_writes` feeds it what `save_snapshot`
actually writes (two sessions, a jump host, a forward) and both fixtures, including the
non-string one: every session loads, schema stays 2, and the old write drops the field. Odd
values: `null`, a number, `true`, an object and an array all load as `None` without quarantine.
A plaintext string is kept opaque, its `Debug` is `<sealed>`, and it opens as `Malformed`.
`otv2:` opens as `NewerFormat`. Test
`odd_sealed_values_load_leniently_and_never_open_as_a_password`. The v0.6.8 release binary was
not run (none exists locally); that remains the owner's check.

**(g) Context binding.**

- Locked with an edited host, port or user: an Info toast, then the Unlock dialog. Cancel leaves
  the file untouched, and Remove then works without the vault (code; walked by the implementer,
  steps 39-42).
- A method change or a cleared username drops the envelope (`password_plan` table test).
- Duplicate keeps the envelope (the implementer's test, plus the copy-delete case in (a)).
- Quick Connect: I5, consistent with the As-built note. It is a deliberate scope choice, not a
  silent gap in the LLD.

**(h) Plaintext hygiene.**

- Added lines contain no `log::` / `println!` / `{:?}` of a password or envelope. The only new
  logs are the non-string warning (no value) and "vault.json was reset or replaced elsewhere".
- The typed password reaches a `SecretString` once (`auth_form.rs:199-201`).
- A saved password is opened only in `take_auth` and goes straight into
  `SshAuthMethod::Password`.
- The password kept for "Save password" is a zeroizing clone held by the `on_connected`
  closure. It is dropped with the request.
- `SavedPassword` / `SshSession` `Debug` show `otv1:<sealed>`.
- No toast text interpolates a secret.
- Walk: none of the four passwords (two SSH, the changed one, the master) appears in UTF-8 or
  UTF-16 in any file of the private profile (including `.bak`, `known_hosts` and `crashes/`),
  in the app logs or in the dev-server logs, and no `otv1:` appears in any log. The search was
  checked by finding the word `bastion` (2 files).

**(i) Dev server.** `--password` rejects every other password (walked: "rejected" in the log).
`direct-tcpip` relays only `127.0.0.1`, `localhost` (I6) or `::1`, and refuses any other host
and any port above 65535. The echo is now keyed on `channels`, which holds only shell session
channels: the SFTP subsystem removes its own channel, and a relayed channel is moved into the
stream and never inserted. So relayed bytes are not echoed. Walked: the two-hop chain completed
through the relay.

## GUI / E2E spot check

The worktree's existing debug `oneterm.exe` (built 16:26, after the last code commit 16:25; the
later commits touch only `crates/tools` and docs), with a private `USERPROFILE` and working
directory (`us0158v-walk`), touched only the pid it launched. Two loopback dev servers:
`127.0.0.1:2232 --password bv-alpha-7731` (session "bastion", user `ops`) and
`127.0.0.1:2233 --password bv-bravo-8842` (session "prod", user `deploy`, jump host bastion).
Screenshots `us0158v-*.png` are in the session scratchpad.

| Step | Result |
| --- | --- |
| 3-4 bastion Properties, box ticked, empty password, Save | toast "Enter the password to save, or untick Save password."; nothing written |
| 5 typed password, Save | "Create a master password" stacked |
| 6 click outside the Create dialog (on the session dialog's Save) | the Create dialog is dismissed; the Save underneath is **not** triggered; no `vault.json` |
| 9-11 Save, master password twice, Enter | both dialogs close; `ssh_session.json` holds `sealed_password: otv1:...` for bastion; `vault.json` created |
| 12-13 prod Properties, password, tick, Save (unlocked) | saved with no prompt; two envelopes in the file |
| 14-15 restart, Open prod | Unlock dialog stacked on the connect dialog |
| 16-19 master password, Enter, Enter | unlocked, then connected: two host keys trusted, both servers log `accepted`, bastion logs `direct-tcpip to 127.0.0.1:2233: relaying`, tab + SFTP open. **Two hops, nothing typed.** |
| 20 Open prod again (unlocked) | both rows "Using saved password", no prompt |
| 21-22 target server restarted with a new password (new host key), Enter, Trust | toast "SSH authentication failed: rejected by the server ..."; the connect dialog had already closed for the host-key alert (D2) |
| 23 Open prod, Enter | target row: empty input, "Update saved password" ticked, "The server rejected the saved password. Type the current one."; hop row still saved; `ssh_session.json` byte-identical |
| 24 new password, Enter | connected; old envelope gone from `ssh_session.json` and `.bak`; `.bak` = `.json`; two envelopes remain |
| 25-28 Settings > Security > Reset, `RESET` | `vault.json` / `vault.bak` gone; `ssh_session.json` and `.bak`: 0 `sealed_password`, 0 `otv`, 2 sessions |
| plaintext search | none (see (h)) |

Not walked here (the implementer's walk covers them): the host edit while unlocked or locked,
Duplicate, and a Save-password from the connect dialog on a first typed password (an owner gap
in the packet as well).

## Tests added

All in `crates/session-ui/src/us0158_verify_tests.rs` (a child of `session_state`, declared at
`session_state.rs` before `persistence_tests`). All pass:

- `deleting_a_session_scrubs_only_when_its_envelope_leaves_the_document`
- `the_scrub_is_decided_against_the_replaced_file_not_the_last_snapshot`
- `a_stale_backup_survives_the_first_write_after_the_document_went_missing` (I1, pinned)
- `the_old_flattened_entry_reads_what_the_current_binary_writes`
- `odd_sealed_values_load_leniently_and_never_open_as_a_password`
- `hop_attribution::ports_sharing_a_prefix_and_ipv6_hosts_pick_their_own_hop` (gpui)
- `hop_attribution::d1_an_ipv6_hop_whose_address_extends_another_is_misattributed` (gpui; D1
  pinned, flip to `Some(1)` when fixed)

## The LLD "As built" departures, judged

| Departure | Judgement |
| --- | --- |
| Form state as `Rc` cells instead of a `PasswordSource` enum | Fine. It is the smaller change, and the cells are shared by the clones the dialogs need. |
| The US-0157 Unlock dialog unchanged (no "prod (and 1 jump host)" line) | Fine for now; cosmetic. |
| Session-dialog messages as toasts; no note after a cancelled Set up / Unlock | Acceptable. LLD § 2's inline "Not saved: a master password is needed." is missing; the ticked box stays, so nothing is lost. |
| Reseal without a vault: Uninitialised drops with a note, Unavailable refuses | Correct. An envelope that cannot be opened must not be carried forward unverified (LLD § 3). |
| Hop failure by the `jump host user@host:port:` prefix, not a structured index | Works for the common cases. D1 is the cost the LLD predicted ("a structured hop index is preferred"). |
| `on_connected` / `on_failed` signatures widened | Fine. Every caller was updated (Quick Connect included). |
| `VaultState::seal` on the UI thread, `ResetElsewhere`, emits `Locked` | Correct per § 8. **US-0159 must know:** `VaultEvent::Locked` is now also emitted by a refused seal, not only by Lock Now or the idle lock, and the refusal adopts the header now on disk (a second re-read path after the `AlreadyInitialised` one in § 8). The lock screen must not treat every `Locked` as "the user locked". |
| `.bak` scrub decided inside `atomic_write_sealed` from the replaced document | Better than the LLD's "flag on the call". It is what makes the coalescing queue and session deletes safe. I1 is its only edge. |
| `lenient_sealed` warns without the session id | Acceptable; a field deserializer cannot see the id. |
| Connect-dialog save refused while the vault is Unavailable | Consistent with the session dialog. |
| Quick Connect uses a hop's saved password only while unlocked | An extension beyond LLD § 7. I5 lists its two rough edges. |
| `set_sealed_password` refuses an address or method change made meanwhile | Correct; it prevents binding a password to the wrong address. |
| Dev server `--password` and loopback `direct-tcpip`, echo only shell channels | Correct (attack i). |

## For the owner / US-0159

- D1-D4 are Low. They can ship and be fixed as follow-ups, or D3 and D4 can be fixed in a short
  rework (a few lines each).
- The owner checks that remain open from the packet: the v0.6.8 release binary against a
  document with `sealed_password`; Linux / macOS `0600` (`cfg(unix)` test plus review only); a
  real OpenSSH host for the rejection wording and a real chain.
- US-0159: see the `Locked`-event row above. The idle lock hides an open connect dialog, and at
  Connect `take_auth` turns a saved row into an input when the vault is locked ("Saved
  passwords are locked."). The LLD wants the rows to stay saved after re-unlock, which needs
  the dialog to re-run `use_saved(true)` on `Unlocked`.
