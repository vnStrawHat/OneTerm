use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui::{AppContext as _, Entity, TestAppContext};
use oneterm_core::SecretString;
use oneterm_vault::{KdfParams, VaultError};

use super::*;

/// Made by an implementation independent of OneTerm, at `KdfParams::FLOOR`
/// (see `crates/vault/src/document_tests.rs`), so it needs a rehash.
const V1: &str = include_str!("../../vault/tests/fixtures/persistence/vault-v1.json");
const V1_PASSWORD: &str = "correct horse battery staple";
const V9: &str = include_str!("../../vault/tests/fixtures/persistence/vault-v9.json");

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Removes the directory on drop, including when an assertion panics
/// (`BUG-0083`).
struct TemporaryDirectory(PathBuf);

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        // Best effort: nothing in a destructor can report a failed cleanup.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for TemporaryDirectory {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

fn test_dir() -> TemporaryDirectory {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "oneterm-vault-state-{}-{nonce}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).unwrap();
    TemporaryDirectory(path)
}

fn secret(text: &str) -> SecretString {
    SecretString::new(text)
}

fn read(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name)).ok()
}

/// A directory holding the `vault-v1.json` fixture.
fn fixture_dir() -> TemporaryDirectory {
    let dir = test_dir();
    std::fs::write(dir.join("vault.json"), V1).unwrap();
    dir
}

fn state_in(dir: &Path, read_only: bool, cx: &mut TestAppContext) -> Entity<VaultState> {
    cx.new(|_| VaultState::load(dir.to_path_buf(), read_only))
}

fn record_events(
    state: &Entity<VaultState>,
    cx: &mut TestAppContext,
) -> Rc<RefCell<Vec<VaultEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|cx| {
        cx.subscribe(state, move |_, event: &VaultEvent, _| {
            sink.borrow_mut().push(*event)
        })
        .detach();
    });
    events
}

fn phase(state: &Entity<VaultState>, cx: &mut TestAppContext) -> VaultPhase {
    state.read_with(cx, |state, _| state.phase())
}

async fn unlock(
    state: &Entity<VaultState>,
    password: &str,
    cx: &mut TestAppContext,
) -> Result<(), VaultActionError> {
    let task = state.update(cx, |state, cx| state.unlock(secret(password), cx));
    let result = task.await;
    // Let a rehash started by the unlock finish.
    cx.run_until_parked();
    result
}

#[test]
fn the_backoff_starts_at_the_fifth_failure_and_caps_at_thirty_seconds() {
    for failures in 0..=4 {
        assert_eq!(backoff_after(failures), None, "{failures}");
    }
    let seconds: Vec<u64> = (5..=10)
        .map(|failures| backoff_after(failures).unwrap().as_secs())
        .collect();
    assert_eq!(seconds, [2, 4, 8, 16, 30, 30]);
    assert_eq!(backoff_after(200), Some(Duration::from_secs(30)));
    assert_eq!(backoff_after(u32::MAX), Some(Duration::from_secs(30)));
}

#[test]
fn the_retry_message_never_reads_zero_seconds() {
    assert!(retry_message(Duration::from_millis(1)).contains("in 1 s"));
    assert!(retry_message(Duration::from_millis(1500)).contains("in 2 s"));
    assert!(retry_message(Duration::from_secs(30)).contains("in 30 s"));
}

#[test]
fn only_the_three_refusals_skip_the_rehash_silently() {
    assert!(is_expected_rehash_refusal(&VaultError::ReadOnly));
    assert!(is_expected_rehash_refusal(&VaultError::VaultReplaced));
    assert!(is_expected_rehash_refusal(&VaultError::ChangedElsewhere));
    assert!(!is_expected_rehash_refusal(&VaultError::WrongPassword));
    assert!(!is_expected_rehash_refusal(&VaultError::Randomness));
}

#[gpui::test]
async fn set_up_lock_and_unlock_walk_the_phases(cx: &mut TestAppContext) {
    let dir = test_dir();
    let state = state_in(&dir, false, cx);
    let events = record_events(&state, cx);
    assert_eq!(phase(&state, cx), VaultPhase::Uninitialised);
    state.read_with(cx, |state, _| {
        assert!(!state.can_change());
        assert!(!state.can_reset());
    });

    let task = state.update(cx, |state, cx| {
        state.set_up(secret("first master password"), cx)
    });
    assert!(state.read_with(cx, |state, _| state.is_busy()));
    task.await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
    let raw = read(&dir, "vault.json").unwrap();
    assert!(!raw.contains("first master password"));
    assert!(raw.contains("\"memory_kib\""));
    assert!(raw.contains("\"argon2id\""));

    state.update(cx, |state, cx| state.lock(cx));
    assert_eq!(phase(&state, cx), VaultPhase::Locked);
    assert!(state.read_with(cx, |state, _| state.unlocked().is_none()));

    unlock(&state, "first master password", cx).await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
    assert_eq!(
        *events.borrow(),
        [
            VaultEvent::Initialised,
            VaultEvent::Locked,
            VaultEvent::Unlocked
        ]
    );
}

#[gpui::test]
async fn wrong_passwords_back_off_from_the_fifth_and_a_success_resets_the_count(
    cx: &mut TestAppContext,
) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    for attempt in 1..=4 {
        let error = unlock(&state, "wrong", cx).await.unwrap_err();
        assert!(matches!(
            error,
            VaultActionError::Vault(VaultError::WrongPassword)
        ));
        assert_eq!(
            state.read_with(cx, |state, _| state.retry_after()),
            None,
            "attempt {attempt}"
        );
    }
    unlock(&state, "wrong", cx).await.unwrap_err();
    let wait = state.read_with(cx, |state, _| state.retry_after()).unwrap();
    assert!(wait <= Duration::from_secs(2));

    // While backing off, even the right password is not tried.
    let error = unlock(&state, V1_PASSWORD, cx).await.unwrap_err();
    assert!(matches!(error, VaultActionError::RetryIn(_)));
    assert_eq!(phase(&state, cx), VaultPhase::Locked);

    // Expire the wait instead of sleeping through it.
    state.update(cx, |state, _| state.retry_at = Some(Instant::now()));
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    state.read_with(cx, |state, _| {
        assert_eq!(state.phase(), VaultPhase::Unlocked);
        assert_eq!(state.failures, 0);
        assert_eq!(state.retry_after(), None);
    });
}

#[gpui::test]
async fn an_unlock_below_the_defaults_rehashes_in_the_background(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    unlock(&state, V1_PASSWORD, cx).await.unwrap();

    let raw = read(&dir, "vault.json").unwrap();
    assert_ne!(raw, V1, "the header was rewrapped");
    assert_eq!(read(&dir, "vault.bak").as_deref(), Some(raw.as_str()));
    assert!(!state.read_with(cx, |state, _| state.is_busy()));

    // The state holds the new header: a later change is not refused as
    // "changed elsewhere", and the password still unlocks after a lock.
    state.update(cx, |state, cx| state.lock(cx));
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    let task = state.update(cx, |state, cx| {
        state.change_password(secret(V1_PASSWORD), secret("another password"), cx)
    });
    task.await.unwrap();
}

#[gpui::test]
async fn locked_never_rereads_the_file_and_a_missing_file_skips_the_rehash(
    cx: &mut TestAppContext,
) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    std::fs::remove_file(dir.join("vault.json")).unwrap();

    // Verified against the header read at start, not the (missing) file.
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
    // The rehash met `VaultReplaced` and wrote nothing.
    assert!(!dir.join("vault.json").exists());
    assert!(!state.read_with(cx, |state, _| state.is_busy()));
}

#[gpui::test]
async fn a_passer_by_vault_neither_unlocks_nor_blocks_the_owner(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    std::fs::remove_file(dir.join("vault.json")).unwrap();
    oneterm_vault::init_with_params(&dir, &secret("passer-by password"), KdfParams::FLOOR).unwrap();
    let passer_by = read(&dir, "vault.json").unwrap();

    let error = unlock(&state, "passer-by password", cx).await.unwrap_err();
    assert!(matches!(
        error,
        VaultActionError::Vault(VaultError::WrongPassword)
    ));
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    // The rehash saw another vault and left the file alone.
    assert_eq!(read(&dir, "vault.json").unwrap(), passer_by);
}

#[gpui::test]
async fn a_header_rewrapped_elsewhere_skips_the_rehash(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    // Another process changes the password after this one read the header.
    let VaultStatus::Ready(header) = oneterm_vault::load(&dir).unwrap() else {
        panic!("fixture must load");
    };
    let mut other = oneterm_vault::unlock(&header, &secret(V1_PASSWORD)).unwrap();
    other
        .change_password_with_params(
            &dir,
            &secret(V1_PASSWORD),
            &secret("changed elsewhere"),
            KdfParams::FLOOR,
        )
        .unwrap();
    let changed = read(&dir, "vault.json").unwrap();

    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
    assert_eq!(read(&dir, "vault.json").unwrap(), changed);
    assert!(!state.read_with(cx, |state, _| state.is_busy()));
}

#[gpui::test]
async fn change_password_checks_the_old_one_and_scrubs_the_backup(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    let events = record_events(&state, cx);

    let task = state.update(cx, |state, cx| {
        state.change_password(secret("not it"), secret("new master password"), cx)
    });
    let error = task.await.unwrap_err();
    assert!(matches!(
        error,
        VaultActionError::Vault(VaultError::WrongPassword)
    ));
    assert_eq!(state.read_with(cx, |state, _| state.failures), 1);
    assert_eq!(read(&dir, "vault.json").unwrap(), V1);

    // From Locked: a right old password also unlocks.
    let task = state.update(cx, |state, cx| {
        state.change_password(secret(V1_PASSWORD), secret("new master password"), cx)
    });
    task.await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
    assert_eq!(*events.borrow(), [VaultEvent::Unlocked]);
    let raw = read(&dir, "vault.json").unwrap();
    assert_eq!(read(&dir, "vault.bak").unwrap(), raw);

    state.update(cx, |state, cx| state.lock(cx));
    unlock(&state, V1_PASSWORD, cx).await.unwrap_err();
    unlock(&state, "new master password", cx).await.unwrap();
}

#[gpui::test]
async fn reset_announces_itself_first_then_removes_both_files(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    assert!(dir.join("vault.bak").exists());
    let events = record_events(&state, cx);

    let task = state.update(cx, |state, cx| state.reset(cx));
    // The key is dropped at once, before the file goes.
    assert!(state.read_with(cx, |state, _| state.unlocked().is_none()));
    assert_eq!(*events.borrow(), [VaultEvent::Reset]);
    task.await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Uninitialised);
    assert!(!dir.join("vault.json").exists());
    assert!(!dir.join("vault.bak").exists());
}

#[gpui::test]
async fn the_elevated_window_unlocks_but_never_writes(cx: &mut TestAppContext) {
    let empty = test_dir();
    let state = state_in(&empty, true, cx);
    let task = state.update(cx, |state, cx| state.set_up(secret("some password"), cx));
    let error = task.await.unwrap_err();
    assert!(matches!(
        error,
        VaultActionError::Vault(VaultError::ReadOnly)
    ));
    assert!(!empty.join("vault.json").exists());

    let dir = fixture_dir();
    let state = state_in(&dir, true, cx);
    state.read_with(cx, |state, _| {
        assert!(state.is_read_only());
        assert!(!state.can_change());
        assert!(!state.can_reset());
    });
    // Unlock is allowed (intake Q5); the rehash is not attempted.
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
    assert_eq!(read(&dir, "vault.json").unwrap(), V1);
    assert!(!dir.join("vault.bak").exists());

    let task = state.update(cx, |state, cx| {
        state.change_password(secret(V1_PASSWORD), secret("other password"), cx)
    });
    assert!(matches!(
        task.await.unwrap_err(),
        VaultActionError::Vault(VaultError::ReadOnly)
    ));
    let task = state.update(cx, |state, cx| state.reset(cx));
    assert!(matches!(
        task.await.unwrap_err(),
        VaultActionError::Vault(VaultError::ReadOnly)
    ));
    assert_eq!(read(&dir, "vault.json").unwrap(), V1);
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
}

#[gpui::test]
fn damaged_and_newer_files_are_unavailable_and_only_damaged_resets(cx: &mut TestAppContext) {
    let damaged = test_dir();
    std::fs::write(damaged.join("vault.json"), "not json").unwrap();
    let state = state_in(&damaged, false, cx);
    state.read_with(cx, |state, _| {
        assert_eq!(state.phase(), VaultPhase::Unavailable);
        assert!(state.unavailable_reason().unwrap().contains("damaged"));
        assert!(state.can_reset());
        assert!(!state.can_change());
    });

    let newer = test_dir();
    std::fs::write(newer.join("vault.json"), V9).unwrap();
    let state = state_in(&newer, false, cx);
    state.read_with(cx, |state, _| {
        assert_eq!(state.phase(), VaultPhase::Unavailable);
        assert!(state.unavailable_reason().unwrap().contains("newer"));
        assert!(!state.can_reset());
    });
    // Loading never touched either file.
    assert_eq!(read(&damaged, "vault.json").unwrap(), "not json");
    assert_eq!(read(&newer, "vault.json").unwrap(), V9);
}

// Independent verification of `US-0157` (`evidence/US-0157-verify.md`). Tests
// named `verifier_d<N>_...` pin the behaviour of a reported defect as it is
// today; invert them when the defect is fixed.

/// Compile-time proof that `VaultState`, which holds the data key while
/// unlocked, is neither `Debug` nor `Clone` (the `assert_not_impl_any`
/// technique `crates/vault/src/vault_tests.rs` uses for `UnlockedVault`).
const _: fn() = || {
    trait AmbiguousIfImpl<A> {
        fn some_item() {}
    }
    impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
    struct IfClone;
    impl<T: Clone> AmbiguousIfImpl<IfClone> for T {}
    struct IfDebug;
    impl<T: ?Sized + std::fmt::Debug> AmbiguousIfImpl<IfDebug> for T {}
    let _ = <VaultState as AmbiguousIfImpl<_>>::some_item;
};

/// Defect D1: an operation's `Task` dropped before it completes cancels the
/// step that clears `busy`, so every later set-up, unlock, change and reset of
/// this process answers `Busy` until restart.
#[gpui::test]
async fn verifier_d1_a_dropped_operation_task_still_applies(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    let task = state.update(cx, |state, cx| state.unlock(secret(V1_PASSWORD), cx));
    drop(task);
    cx.run_until_parked();
    // The unlock (and the rehash it started) applied without anyone awaiting.
    state.read_with(cx, |state, _| {
        assert!(!state.is_busy(), "busy is cleared by the state's own step");
        assert_eq!(state.phase(), VaultPhase::Unlocked);
    });
    state.update(cx, |state, cx| state.lock(cx));
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
}

/// Lock while a rehash runs: allowed; set-up, unlock, change and reset are
/// `Busy`; the finishing rehash leaves the process locked on the new header.
#[gpui::test]
async fn verifier_lock_during_a_rehash_keeps_the_new_header(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    let VaultStatus::Ready(header) = oneterm_vault::load(&dir).unwrap() else {
        panic!("fixture must load");
    };
    let vault = oneterm_vault::unlock(&header, &secret(V1_PASSWORD)).unwrap();
    state.update(cx, |state, cx| {
        state.phase = Phase::Unlocked(Box::new(vault));
        state.rehash(secret(V1_PASSWORD), cx);
        assert!(state.is_busy());
        state.lock(cx);
        assert_eq!(state.phase(), VaultPhase::Locked);
    });
    let change = state.update(cx, |state, cx| {
        state.change_password(secret(V1_PASSWORD), secret("another password"), cx)
    });
    assert!(matches!(change.await, Err(VaultActionError::Busy)));
    let reset = state.update(cx, |state, cx| state.reset(cx));
    assert!(matches!(reset.await, Err(VaultActionError::Busy)));
    let again = state.update(cx, |state, cx| state.unlock(secret(V1_PASSWORD), cx));
    assert!(matches!(again.await, Err(VaultActionError::Busy)));

    cx.run_until_parked();
    state.read_with(cx, |state, _| {
        assert!(!state.is_busy());
        assert_eq!(state.phase(), VaultPhase::Locked);
    });
    assert_ne!(
        read(&dir, "vault.json").unwrap(),
        V1,
        "the rehash was written"
    );
    // The held header is the rewritten one: unlock works and a change is not
    // refused as "changed elsewhere".
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    let change = state.update(cx, |state, cx| {
        state.change_password(secret(V1_PASSWORD), secret("another password"), cx)
    });
    change.await.unwrap();
}

/// Defect D2: "Lock now" while a change runs is undone when the change
/// finishes: the key is back in memory and `Unlocked` is emitted, with no
/// password typed after the lock. `US-0159`'s idle lock calls the same path.
#[gpui::test]
async fn verifier_d2_a_lock_during_a_change_wins(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    let events = record_events(&state, cx);
    let task = state.update(cx, |state, cx| {
        let task = state.change_password(secret(V1_PASSWORD), secret("new master password"), cx);
        state.lock(cx);
        assert_eq!(state.phase(), VaultPhase::Locked);
        task
    });
    task.await.unwrap();
    // The change happened, but no key came back.
    assert_eq!(phase(&state, cx), VaultPhase::Locked);
    assert_eq!(*events.borrow(), [VaultEvent::Locked]);
    assert_eq!(read(&dir, "vault.bak"), read(&dir, "vault.json"));
    // The held header is the new one: the new password unlocks, the old not.
    unlock(&state, V1_PASSWORD, cx).await.unwrap_err();
    unlock(&state, "new master password", cx).await.unwrap();
}

#[gpui::test]
async fn a_lock_during_an_unlock_wins(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    let events = record_events(&state, cx);
    let task = state.update(cx, |state, cx| {
        let task = state.unlock(secret(V1_PASSWORD), cx);
        state.lock(cx);
        task
    });
    let error = task.await.unwrap_err();
    assert!(matches!(error, VaultActionError::LockedMeanwhile));
    cx.run_until_parked();
    state.read_with(cx, |state, _| {
        assert_eq!(state.phase(), VaultPhase::Locked);
        assert!(state.unlocked().is_none());
        assert!(!state.is_busy());
        // The right password still counts as a success.
        assert_eq!(state.failures, 0);
    });
    assert!(events.borrow().is_empty());
    // No rehash ran: the fixture is untouched.
    assert_eq!(read(&dir, "vault.json").unwrap(), V1);
}

#[gpui::test]
async fn a_lock_during_a_set_up_wins(cx: &mut TestAppContext) {
    let dir = test_dir();
    let state = state_in(&dir, false, cx);
    let events = record_events(&state, cx);
    let task = state.update(cx, |state, cx| {
        let task = state.set_up(secret("first master password"), cx);
        state.lock(cx);
        task
    });
    task.await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Locked);
    assert_eq!(
        *events.borrow(),
        [VaultEvent::Initialised, VaultEvent::Locked]
    );
    unlock(&state, "first master password", cx).await.unwrap();
}

/// Another process set up first: the state leaves Uninitialised for what the
/// file now says (Locked) and unlocks with that process's password.
#[gpui::test]
async fn verifier_set_up_after_another_process_recovers_to_locked(cx: &mut TestAppContext) {
    let dir = test_dir();
    let state = state_in(&dir, false, cx);
    oneterm_vault::init_with_params(&dir, &secret("other process password"), KdfParams::FLOOR)
        .unwrap();
    let task = state.update(cx, |state, cx| state.set_up(secret("my own password"), cx));
    assert!(matches!(
        task.await.unwrap_err(),
        VaultActionError::Vault(VaultError::AlreadyInitialised)
    ));
    state.read_with(cx, |state, _| {
        assert_eq!(state.phase(), VaultPhase::Locked);
        assert!(!state.is_busy());
    });
    unlock(&state, "other process password", cx).await.unwrap();
}

/// A set-up whose dialog was cancelled still completes; a second set-up while
/// it runs is `Busy`, and one after it is `WrongPhase`.
#[gpui::test]
async fn verifier_a_second_set_up_is_busy_then_wrong_phase(cx: &mut TestAppContext) {
    let dir = test_dir();
    let state = state_in(&dir, false, cx);
    let first = state.update(cx, |state, cx| state.set_up(secret("first password"), cx));
    let second = state.update(cx, |state, cx| state.set_up(secret("second password"), cx));
    assert!(matches!(second.await, Err(VaultActionError::Busy)));
    first.await.unwrap();
    let third = state.update(cx, |state, cx| state.set_up(secret("third password"), cx));
    assert!(matches!(third.await, Err(VaultActionError::WrongPhase)));
    state.update(cx, |state, cx| state.lock(cx));
    unlock(&state, "second password", cx).await.unwrap_err();
    unlock(&state, "first password", cx).await.unwrap();
}

/// Wrong old passwords in Change and wrong unlocks share one count.
#[gpui::test]
async fn verifier_change_and_unlock_share_the_failure_count(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    for _ in 0..4 {
        unlock(&state, "wrong", cx).await.unwrap_err();
    }
    let task = state.update(cx, |state, cx| {
        state.change_password(secret("wrong"), secret("new master password"), cx)
    });
    task.await.unwrap_err();
    assert!(
        state
            .read_with(cx, |state, _| state.retry_after())
            .is_some()
    );
    let task = state.update(cx, |state, cx| {
        state.change_password(secret(V1_PASSWORD), secret("new master password"), cx)
    });
    assert!(matches!(task.await, Err(VaultActionError::RetryIn(_))));
    assert_eq!(read(&dir, "vault.json").unwrap(), V1);
}

/// A reset whose removal fails has already announced `Reset` (owners wiped
/// their sealed values) and dropped the key; `vault.json` stays, the state is
/// Locked, and a retry completes the reset.
#[cfg(windows)]
#[gpui::test]
async fn verifier_a_failed_reset_has_already_announced_itself(cx: &mut TestAppContext) {
    use std::os::windows::fs::OpenOptionsExt as _;

    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    let events = record_events(&state, cx);
    // No sharing: the removal fails with a sharing violation.
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(dir.join("vault.json"))
        .unwrap();
    let task = state.update(cx, |state, cx| state.reset(cx));
    assert!(matches!(
        task.await.unwrap_err(),
        VaultActionError::Vault(VaultError::Io(_))
    ));
    assert_eq!(*events.borrow(), [VaultEvent::Reset]);
    assert_eq!(phase(&state, cx), VaultPhase::Locked);
    drop(held);
    assert!(dir.join("vault.json").exists());

    let task = state.update(cx, |state, cx| state.reset(cx));
    task.await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Uninitialised);
    assert!(!dir.join("vault.json").exists());
    assert!(!dir.join("vault.bak").exists());
}

// Re-verification of the `US-0157` rework (`6db052a4`).

/// Dropped set-up and unlock tasks, one after the other: each still applies
/// and clears `busy`, and the events are the ones an awaited run emits.
#[gpui::test]
async fn verifier_r_dropped_tasks_in_a_row_all_apply(cx: &mut TestAppContext) {
    let dir = test_dir();
    let state = state_in(&dir, false, cx);
    let events = record_events(&state, cx);
    drop(state.update(cx, |state, cx| {
        state.set_up(secret("first master password"), cx)
    }));
    cx.run_until_parked();
    state.update(cx, |state, cx| {
        assert!(!state.is_busy());
        assert_eq!(state.phase(), VaultPhase::Unlocked);
        state.lock(cx);
    });
    drop(state.update(cx, |state, cx| {
        state.unlock(secret("first master password"), cx)
    }));
    cx.run_until_parked();
    state.read_with(cx, |state, _| {
        assert!(!state.is_busy());
        assert_eq!(state.phase(), VaultPhase::Unlocked);
    });
    assert_eq!(
        *events.borrow(),
        [
            VaultEvent::Initialised,
            VaultEvent::Locked,
            VaultEvent::Unlocked
        ]
    );
}

/// A lock with nothing running sets no flag, so the next operation is not
/// treated as "locked meanwhile".
#[gpui::test]
async fn verifier_r_an_idle_lock_does_not_carry_over(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    state.update(cx, |state, cx| {
        state.lock(cx);
        assert!(!state.locked_meanwhile);
    });
    // Also a lock while Locked and idle.
    state.update(cx, |state, cx| state.lock(cx));
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
}

/// Back to back: an unlock locked meanwhile, then a plain unlock, which
/// succeeds (`begin` clears the flag).
#[gpui::test]
async fn verifier_r_the_operation_after_a_locked_one_is_not_refused(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    let task = state.update(cx, |state, cx| {
        let task = state.unlock(secret(V1_PASSWORD), cx);
        state.lock(cx);
        task
    });
    assert!(matches!(task.await, Err(VaultActionError::LockedMeanwhile)));
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
}

/// A wrong password during an unlock that was locked meanwhile still counts
/// as a failure and answers `WrongPassword`, not `LockedMeanwhile`.
#[gpui::test]
async fn verifier_r_a_wrong_password_locked_meanwhile_still_counts(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    let task = state.update(cx, |state, cx| {
        let task = state.unlock(secret("wrong"), cx);
        state.lock(cx);
        task
    });
    assert!(matches!(
        task.await,
        Err(VaultActionError::Vault(VaultError::WrongPassword))
    ));
    state.read_with(cx, |state, _| {
        assert_eq!(state.failures, 1);
        assert!(!state.is_busy());
        assert_eq!(state.phase(), VaultPhase::Locked);
    });
}

/// A change started from Locked and locked meanwhile: the file changes, the
/// process stays Locked on the new header, and no event is emitted.
#[gpui::test]
async fn verifier_r_a_change_from_locked_locked_meanwhile_stays_locked(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    let events = record_events(&state, cx);
    let task = state.update(cx, |state, cx| {
        let task = state.change_password(secret(V1_PASSWORD), secret("new master password"), cx);
        state.lock(cx);
        task
    });
    task.await.unwrap();
    state.read_with(cx, |state, _| {
        assert_eq!(state.phase(), VaultPhase::Locked);
        assert!(state.unlocked().is_none());
    });
    assert!(events.borrow().is_empty());
    assert_ne!(read(&dir, "vault.json").unwrap(), V1);
    unlock(&state, V1_PASSWORD, cx).await.unwrap_err();
    unlock(&state, "new master password", cx).await.unwrap();
}

fn ssh_context() -> oneterm_vault::SealContext<'static> {
    oneterm_vault::SealContext {
        host: "10.0.0.1",
        port: 22,
        username: "deploy",
    }
}

fn seal(
    state: &Entity<VaultState>,
    cx: &mut TestAppContext,
) -> Result<oneterm_vault::SealedSecret, VaultActionError> {
    state.update(cx, |state, cx| {
        state.seal(
            oneterm_vault::SecretPurpose::SshPassword,
            &ssh_context(),
            &secret("ssh password"),
            cx,
        )
    })
}

/// `US-0158`: a seal re-reads `vault.json` and seals only under the vault on
/// disk; the sealed value opens with the same context.
#[gpui::test]
async fn a_seal_under_the_vault_on_disk_opens_again(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    assert!(matches!(
        seal(&state, cx),
        Err(VaultActionError::WrongPhase)
    ));
    unlock(&state, V1_PASSWORD, cx).await.unwrap();

    let sealed = seal(&state, cx).unwrap();
    assert!(!format!("{sealed:?}").contains("ssh password"));
    state.read_with(cx, |state, _| {
        let opened = state
            .unlocked()
            .unwrap()
            .open(
                oneterm_vault::SecretPurpose::SshPassword,
                &ssh_context(),
                &sealed,
            )
            .unwrap();
        assert_eq!(opened.expose_secret(), "ssh password");
    });
}

/// `US-0158` (`persistence-and-migration.md` § 8): another process reset the
/// vault and set up a new one, or only reset it. The seal is refused, the key
/// is dropped, and the phase follows the file.
#[gpui::test]
async fn a_seal_after_a_reset_elsewhere_is_refused_and_drops_the_key(cx: &mut TestAppContext) {
    for replaced in [true, false] {
        let dir = fixture_dir();
        let state = state_in(&dir, false, cx);
        unlock(&state, V1_PASSWORD, cx).await.unwrap();
        let events = record_events(&state, cx);

        std::fs::remove_file(dir.join("vault.json")).unwrap();
        if replaced {
            let other = test_dir();
            drop(
                oneterm_vault::init_with_params(&other, &secret("other one"), KdfParams::FLOOR)
                    .unwrap(),
            );
            std::fs::copy(other.join("vault.json"), dir.join("vault.json")).unwrap();
        }
        assert!(matches!(
            seal(&state, cx),
            Err(VaultActionError::ResetElsewhere)
        ));
        let expected = if replaced {
            VaultPhase::Locked
        } else {
            VaultPhase::Uninitialised
        };
        assert_eq!(phase(&state, cx), expected);
        assert!(state.read_with(cx, |state, _| state.unlocked().is_none()));
        assert_eq!(*events.borrow(), [VaultEvent::Locked]);
    }
}

// ── Screen lock (`US-0159`) ─────────────────────────────────────────

async fn unlock_screen(
    state: &Entity<VaultState>,
    password: &str,
    cx: &mut TestAppContext,
) -> Result<(), VaultActionError> {
    let task = state.update(cx, |state, cx| state.unlock_screen(secret(password), cx));
    let result = task.await;
    cx.run_until_parked();
    result
}

fn screen_locked(state: &Entity<VaultState>, cx: &mut TestAppContext) -> bool {
    state.read_with(cx, |state, _| state.is_screen_locked())
}

#[gpui::test]
async fn a_screen_lock_drops_the_key_and_only_the_lock_screen_lifts_it(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    let events = record_events(&state, cx);

    state.update(cx, |state, cx| state.lock_screen(cx));
    assert!(screen_locked(&state, cx));
    assert_eq!(phase(&state, cx), VaultPhase::Locked);
    assert!(state.read_with(cx, |state, _| state.unlocked().is_none()));
    assert_eq!(
        *events.borrow(),
        [VaultEvent::Locked, VaultEvent::ScreenLocked]
    );
    // Nothing else opens or changes the vault while the screen is locked.
    state.read_with(cx, |state, _| {
        assert!(!state.can_change());
        assert!(!state.can_reset());
    });
    assert!(matches!(
        unlock(&state, V1_PASSWORD, cx).await.unwrap_err(),
        VaultActionError::WrongPhase
    ));
    let task = state.update(cx, |state, cx| state.reset(cx));
    assert!(matches!(
        task.await.unwrap_err(),
        VaultActionError::WrongPhase
    ));
    let task = state.update(cx, |state, cx| {
        state.change_password(secret(V1_PASSWORD), secret("another password"), cx)
    });
    assert!(matches!(
        task.await.unwrap_err(),
        VaultActionError::WrongPhase
    ));
    assert!(dir.join("vault.json").exists());

    // A second lock is a no-op; a wrong password counts and changes nothing.
    state.update(cx, |state, cx| state.lock_screen(cx));
    assert!(matches!(
        unlock_screen(&state, "wrong", cx).await.unwrap_err(),
        VaultActionError::Vault(VaultError::WrongPassword)
    ));
    assert!(screen_locked(&state, cx));
    assert_eq!(state.read_with(cx, |state, _| state.failures), 1);

    unlock_screen(&state, V1_PASSWORD, cx).await.unwrap();
    assert!(!screen_locked(&state, cx));
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
    assert_eq!(state.read_with(cx, |state, _| state.failures), 0);
    assert_eq!(
        *events.borrow(),
        [
            VaultEvent::Locked,
            VaultEvent::ScreenLocked,
            VaultEvent::Unlocked
        ]
    );
    // Not screen-locked any more: the lock screen's way out is refused.
    assert!(matches!(
        unlock_screen(&state, V1_PASSWORD, cx).await.unwrap_err(),
        VaultActionError::WrongPhase
    ));
}

/// Screen protection does not depend on the user ever having unlocked (HLD).
#[gpui::test]
async fn a_screen_lock_from_locked_unlocks_the_vault_on_the_way_out(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    let events = record_events(&state, cx);
    state.update(cx, |state, cx| state.lock_screen(cx));
    assert!(screen_locked(&state, cx));
    assert_eq!(*events.borrow(), [VaultEvent::ScreenLocked]);
    unlock_screen(&state, V1_PASSWORD, cx).await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
}

#[gpui::test]
fn no_vault_means_no_screen_lock(cx: &mut TestAppContext) {
    let empty = test_dir();
    let state = state_in(&empty, false, cx);
    state.update(cx, |state, cx| state.lock_screen(cx));
    assert!(!screen_locked(&state, cx));

    let damaged = test_dir();
    std::fs::write(damaged.join("vault.json"), "not json").unwrap();
    let state = state_in(&damaged, false, cx);
    assert_eq!(phase(&state, cx), VaultPhase::Unavailable);
    state.update(cx, |state, cx| state.lock_screen(cx));
    assert!(!screen_locked(&state, cx));
}

#[gpui::test]
async fn a_screen_lock_during_a_dialog_unlock_wins(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    let task = state.update(cx, |state, cx| {
        let task = state.unlock(secret(V1_PASSWORD), cx);
        state.lock_screen(cx);
        task
    });
    assert!(matches!(
        task.await.unwrap_err(),
        VaultActionError::LockedMeanwhile
    ));
    cx.run_until_parked();
    assert!(screen_locked(&state, cx));
    assert!(state.read_with(cx, |state, _| state.unlocked().is_none()));
}

#[gpui::test]
async fn a_screen_lock_during_a_change_keeps_only_the_new_header(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    unlock(&state, V1_PASSWORD, cx).await.unwrap();
    let task = state.update(cx, |state, cx| {
        let task = state.change_password(secret(V1_PASSWORD), secret("new master password"), cx);
        state.lock_screen(cx);
        task
    });
    task.await.unwrap();
    assert!(screen_locked(&state, cx));
    assert!(state.read_with(cx, |state, _| state.unlocked().is_none()));
    // The held header is the new one.
    assert!(unlock_screen(&state, V1_PASSWORD, cx).await.is_err());
    unlock_screen(&state, "new master password", cx)
        .await
        .unwrap();
}

/// The lock screen never re-reads `vault.json` (`idle-lock-overlay.md`, edge
/// cases): a passer-by who resets and sets up a vault of their own in another
/// OneTerm cannot open this one.
#[gpui::test]
async fn the_lock_screen_ignores_a_vault_replaced_on_disk(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    state.update(cx, |state, cx| state.lock_screen(cx));
    std::fs::remove_file(dir.join("vault.json")).unwrap();
    oneterm_vault::init_with_params(&dir, &secret("passer-by password"), KdfParams::FLOOR).unwrap();

    assert!(matches!(
        unlock_screen(&state, "passer-by password", cx)
            .await
            .unwrap_err(),
        VaultActionError::Vault(VaultError::WrongPassword)
    ));
    assert!(screen_locked(&state, cx));
    unlock_screen(&state, V1_PASSWORD, cx).await.unwrap();
    assert!(!screen_locked(&state, cx));
}

/// Intake Q5: the elevated window locks and unlocks with the same password,
/// and writes nothing (no rehash of the below-default fixture).
#[gpui::test]
async fn the_elevated_window_locks_and_unlocks_read_only(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, true, cx);
    state.update(cx, |state, cx| state.lock_screen(cx));
    assert!(screen_locked(&state, cx));
    unlock_screen(&state, V1_PASSWORD, cx).await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Unlocked);
    assert_eq!(read(&dir, "vault.json").unwrap(), V1);
    assert!(!dir.join("vault.bak").exists());
}

/// One failure count for the dialogs and the lock screen (LLD § 5).
#[gpui::test]
async fn the_lock_screen_shares_the_dialogs_back_off(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    for _ in 0..4 {
        unlock(&state, "wrong", cx).await.unwrap_err();
    }
    state.update(cx, |state, cx| state.lock_screen(cx));
    unlock_screen(&state, "wrong", cx).await.unwrap_err();
    assert!(
        state
            .read_with(cx, |state, _| state.retry_after())
            .is_some()
    );
    assert!(matches!(
        unlock_screen(&state, V1_PASSWORD, cx).await.unwrap_err(),
        VaultActionError::RetryIn(_)
    ));
    state.update(cx, |state, _| state.retry_at = Some(Instant::now()));
    unlock_screen(&state, V1_PASSWORD, cx).await.unwrap();
}

/// A reset started just before the lock removes the vault, and with it the
/// lock: there is nothing left to unlock with.
#[gpui::test]
async fn a_reset_that_lands_after_a_screen_lock_lifts_it(cx: &mut TestAppContext) {
    let dir = fixture_dir();
    let state = state_in(&dir, false, cx);
    let task = state.update(cx, |state, cx| {
        let task = state.reset(cx);
        state.lock_screen(cx);
        task
    });
    task.await.unwrap();
    assert_eq!(phase(&state, cx), VaultPhase::Uninitialised);
    assert!(!screen_locked(&state, cx));
}
