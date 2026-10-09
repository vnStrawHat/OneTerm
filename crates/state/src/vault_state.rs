//! [`VaultState`] — the process-wide phase of the master-password vault
//! (`IN-0050`, `high-level-design.md` § Lifecycle state machine).
//!
//! `oneterm-vault` owns the crypto and `vault.json`; this global owns what the
//! process holds: nothing (Uninitialised), the header read once at start
//! (Locked), or the data key (Unlocked). Every Argon2id run happens on the
//! background executor, one operation at a time.
//!
//! **Locked never re-reads `vault.json`.** An unlock verifies against the header
//! held in memory, so deleting or replacing the file (a passer-by with a second
//! OneTerm) cannot open this process (`idle-lock-overlay.md`, edge cases). The
//! cost: a password changed in another process is picked up at the next start.
//!
//! Each operation applies its result to the state in a step the state
//! detaches itself; the `Task` a caller receives only carries the answer, so
//! dropping it never leaves the state busy. A lock asked for while an
//! operation runs wins: the operation's result never puts a key back.
//!
//! **Screen lock (`US-0159`).** Two facts make the state: whether the key is in
//! memory ([`VaultPhase`]) and whether every window is covered by the lock
//! screen ([`VaultState::is_screen_locked`]). A screen lock always drops the key
//! too, so it sits beside Locked only; [`VaultState::unlock_screen`] is the one
//! way out, and it too verifies against the header held in memory.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures::channel::oneshot;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use oneterm_core::SecretString;
use oneterm_vault::{
    SealContext, SealedSecret, SecretPurpose, UnlockedVault, VaultError, VaultHeader, VaultStatus,
};

/// The shortest master password set-up and change accept, in characters.
pub const MIN_PASSWORD_CHARS: usize = 8;

/// Consecutive wrong passwords allowed before the back-off starts.
const FREE_ATTEMPTS: u32 = 4;

/// The longest back-off between two attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// What the process holds of the vault, as the UI sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultPhase {
    /// No `vault.json`: no master password is set up.
    Uninitialised,
    /// A vault exists; the key is not in memory.
    Locked,
    /// The key is in memory.
    Unlocked,
    /// `vault.json` is damaged or from a newer OneTerm: nothing is attempted
    /// with it ([`VaultState::unavailable_reason`] says why).
    Unavailable,
}

/// Emitted by [`VaultState`] on each phase change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultEvent {
    /// A master password was set up; the vault is unlocked.
    Initialised,
    /// The key was loaded into memory.
    Unlocked,
    /// The key was dropped. Not only a user's lock: a refused seal emits it
    /// too (`US-0158`), so the lock screen never reacts to it.
    Locked,
    /// Every window is now covered by the lock screen (`US-0159`).
    ScreenLocked,
    /// A reset has started. Emitted **before** `vault.json` is removed: every
    /// owner of sealed values wipes them on it (`oneterm_vault::reset`).
    Reset,
}

/// Why a [`VaultState`] operation did not happen.
#[derive(Debug)]
pub enum VaultActionError {
    /// The vault refused (wrong password, read-only window, file changed ...).
    Vault(VaultError),
    /// Too many wrong passwords in a row: try again after this long.
    RetryIn(Duration),
    /// Another master-password operation is still running.
    Busy,
    /// The operation does not apply to the current phase (another window
    /// changed it first).
    WrongPhase,
    /// The password was right, but OneTerm was locked while it was checked,
    /// and the lock wins.
    LockedMeanwhile,
    /// A seal found `vault.json` reset or replaced by another OneTerm process;
    /// the key was dropped and nothing was sealed.
    ResetElsewhere,
}

impl VaultActionError {
    /// The sentence a dialog shows under its fields.
    pub fn message(&self) -> String {
        match self {
            Self::Vault(VaultError::WrongPassword) => "Wrong master password.".to_owned(),
            Self::Vault(VaultError::EmptyPassword) => "Enter the master password.".to_owned(),
            Self::Vault(VaultError::AlreadyInitialised) => {
                "A master password was already set up in another OneTerm window.".to_owned()
            }
            Self::Vault(VaultError::VaultReplaced) => {
                "The master password was reset or replaced in another OneTerm window. Restart OneTerm to use the current one.".to_owned()
            }
            Self::Vault(VaultError::ChangedElsewhere) => {
                "The master password was changed in another OneTerm window. Restart OneTerm to use it.".to_owned()
            }
            Self::Vault(VaultError::ReadOnly) => {
                "An administrator window cannot change the master password.".to_owned()
            }
            Self::Vault(other) => format!("The master password operation failed: {other}."),
            Self::RetryIn(wait) => retry_message(*wait),
            Self::Busy => "Another master password operation is still running.".to_owned(),
            Self::LockedMeanwhile => {
                "OneTerm was locked while the password was checked. Unlock again.".to_owned()
            }
            Self::ResetElsewhere => {
                "The master password was reset in another OneTerm window. Nothing was saved."
                    .to_owned()
            }
            Self::WrongPhase => {
                "The master password changed in the meantime. Close this dialog and try again."
                    .to_owned()
            }
        }
    }

    pub(crate) fn is_wrong_password(&self) -> bool {
        matches!(self, Self::Vault(VaultError::WrongPassword))
    }
}

/// "Try again in N s.", rounded up so it never reads 0 while still waiting.
pub fn retry_message(wait: Duration) -> String {
    let seconds = wait.as_millis().div_ceil(1000).max(1);
    format!("Too many wrong attempts. Try again in {seconds} s.")
}

/// The wait after `failures` consecutive wrong passwords: none for the first
/// four, then 2, 4, 8, 16 s, then 30 s for every further one. In memory only;
/// never a lockout and never a wipe (`idle-lock-overlay.md` § 5).
pub(crate) fn backoff_after(failures: u32) -> Option<Duration> {
    let over = failures
        .checked_sub(FREE_ATTEMPTS)
        .filter(|over| *over > 0)?;
    let seconds = 1u64.checked_shl(over).unwrap_or(u64::MAX);
    Some(Duration::from_secs(seconds).min(MAX_BACKOFF))
}

/// A refusal that means "this process should not rehash now", not a fault:
/// the window is elevated, or the file is not the header this process holds
/// any more (`US-0156` carry-forward).
pub(crate) fn is_expected_rehash_refusal(error: &VaultError) -> bool {
    matches!(
        error,
        VaultError::ReadOnly | VaultError::VaultReplaced | VaultError::ChangedElsewhere
    )
}

enum Phase {
    Uninitialised,
    Locked(VaultHeader),
    /// Boxed: the cipher's key schedule is over a kilobyte.
    Unlocked(Box<UnlockedVault>),
    Unavailable {
        reason: String,
        resettable: bool,
    },
}

/// The process-wide vault phase, the key while unlocked, and the wrong-password
/// count shared by every window and dialog.
pub struct VaultState {
    dir: PathBuf,
    /// Elevated window (`DEC-0019` M4): no set-up, change, rehash or reset.
    read_only: bool,
    phase: Phase,
    failures: u32,
    retry_at: Option<Instant>,
    /// One master-password operation at a time.
    busy: bool,
    /// A lock was asked for while the running operation ran.
    locked_meanwhile: bool,
    /// Every window shows the lock screen (`US-0159`). Only ever true beside
    /// [`Phase::Locked`]: the key is dropped first.
    screen_locked: bool,
}

impl EventEmitter<VaultEvent> for VaultState {}

/// Global wrapper for `Entity<VaultState>`.
pub struct VaultStateGlobal(pub Entity<VaultState>);

impl Global for VaultStateGlobal {}

impl VaultState {
    /// Read `vault.json` from `config_dir()` and install the global once
    /// (called from `app::init`).
    pub fn init(cx: &mut App) {
        if cx.has_global::<VaultStateGlobal>() {
            return;
        }
        let state = Self::load(
            oneterm_core::config_dir(),
            oneterm_core::elevation::is_restricted(),
        );
        let entity = cx.new(|_| state);
        cx.set_global(VaultStateGlobal(entity));
    }

    /// The global `Entity<VaultState>` (panics if not initialized).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<VaultStateGlobal>().0.clone()
    }

    /// The global `Entity<VaultState>` if initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<VaultStateGlobal>().map(|g| g.0.clone())
    }

    /// The state for `dir/vault.json`, read once now. `read_only` is the
    /// elevated window's switch. [`Self::init`] uses `config_dir()` and
    /// `elevation::is_restricted()`; other crates' tests use a temporary
    /// directory.
    pub fn load(dir: PathBuf, read_only: bool) -> Self {
        let phase = read_phase(&dir);
        Self {
            dir,
            read_only,
            phase,
            failures: 0,
            retry_at: None,
            busy: false,
            locked_meanwhile: false,
            screen_locked: false,
        }
    }

    /// The phase as the UI sees it.
    pub fn phase(&self) -> VaultPhase {
        match self.phase {
            Phase::Uninitialised => VaultPhase::Uninitialised,
            Phase::Locked(_) => VaultPhase::Locked,
            Phase::Unlocked(_) => VaultPhase::Unlocked,
            Phase::Unavailable { .. } => VaultPhase::Unavailable,
        }
    }

    /// Whether every window shows the lock screen (`US-0159`). The phase is
    /// then [`VaultPhase::Locked`]: the key is not in memory.
    pub fn is_screen_locked(&self) -> bool {
        self.screen_locked && matches!(self.phase, Phase::Locked(_))
    }

    /// Whether a vault exists that a screen lock can be lifted with: no vault
    /// (or an unusable one) means no lock (`idle-lock-overlay.md` § 7).
    pub fn can_lock_screen(&self) -> bool {
        matches!(self.phase, Phase::Locked(_) | Phase::Unlocked(_))
    }

    /// Why the vault is [`VaultPhase::Unavailable`].
    pub fn unavailable_reason(&self) -> Option<&str> {
        match &self.phase {
            Phase::Unavailable { reason, .. } => Some(reason),
            _ => None,
        }
    }

    /// Whether this is the elevated window, which never writes `vault.json`.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether a master-password operation is running.
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// Whether Change applies: a vault exists and this window may write it.
    pub fn can_change(&self) -> bool {
        !self.read_only
            && !self.screen_locked
            && matches!(self.phase, Phase::Locked(_) | Phase::Unlocked(_))
    }

    /// Whether Reset applies: a vault exists (or a damaged file waits for it)
    /// and this window may write. A forgotten password cannot be proved, so
    /// Locked qualifies (intake Q7).
    pub fn can_reset(&self) -> bool {
        !self.read_only
            && !self.screen_locked
            && match self.phase {
                Phase::Locked(_) | Phase::Unlocked(_) => true,
                Phase::Unavailable { resettable, .. } => resettable,
                Phase::Uninitialised => false,
            }
    }

    /// How long until the next password attempt is accepted, while backing off.
    pub fn retry_after(&self) -> Option<Duration> {
        self.retry_at
            .and_then(|at| at.checked_duration_since(Instant::now()))
            .filter(|wait| !wait.is_zero())
    }

    /// The unlocked vault, for sealing and opening (`US-0158`).
    pub fn unlocked(&self) -> Option<&UnlockedVault> {
        match &self.phase {
            Phase::Unlocked(vault) => Some(vault),
            _ => None,
        }
    }

    /// Seal `plaintext` for `purpose` at `context` (`US-0158`).
    ///
    /// `vault.json` is re-read first: when it is no longer the vault this
    /// process unlocked (reset or replaced in another OneTerm process), the key
    /// is dropped, the phase follows the file, and the seal is refused, so a
    /// value is never sealed under a vault nobody can open any more
    /// (`persistence-and-migration.md` § 8). Unlock itself never re-reads.
    pub fn seal(
        &mut self,
        purpose: SecretPurpose,
        context: &SealContext<'_>,
        plaintext: &SecretString,
        cx: &mut Context<Self>,
    ) -> Result<SealedSecret, VaultActionError> {
        let Phase::Unlocked(vault) = &self.phase else {
            return Err(VaultActionError::WrongPhase);
        };
        let on_disk = match oneterm_vault::load(&self.dir) {
            Ok(VaultStatus::Ready(header)) => Some(header.vault_id()),
            _ => None,
        };
        if on_disk != Some(vault.vault_id()) {
            log::warn!("vault.json was reset or replaced elsewhere; dropping the key");
            self.phase = read_phase(&self.dir);
            cx.emit(VaultEvent::Locked);
            cx.notify();
            return Err(VaultActionError::ResetElsewhere);
        }
        vault
            .seal(purpose, context, plaintext)
            .map_err(VaultActionError::Vault)
    }

    /// Set up a master password: Uninitialised -> Unlocked (Locked when a lock
    /// was asked for while it ran).
    pub fn set_up(
        &mut self,
        password: SecretString,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), VaultActionError>> {
        if let Err(error) = self.check_write(matches!(self.phase, Phase::Uninitialised)) {
            return Task::ready(Err(error));
        }
        let dir = self.dir.clone();
        self.run(
            cx,
            async move { create_vault(&dir, &password) },
            Self::finish_set_up,
        )
    }

    /// Unlock with `password`, verified against the header held in memory:
    /// Locked -> Unlocked. A header below the default parameters is then
    /// rehashed in the background.
    pub fn unlock(
        &mut self,
        password: SecretString,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), VaultActionError>> {
        let Phase::Locked(header) = &self.phase else {
            return Task::ready(Err(VaultActionError::WrongPhase));
        };
        // The lock screen has its own way out, `unlock_screen`.
        if self.screen_locked {
            return Task::ready(Err(VaultActionError::WrongPhase));
        }
        let header = header.clone();
        if let Err(error) = self.check_attempt() {
            return Task::ready(Err(error));
        }
        self.run(cx, unlock_work(header, password), Self::finish_unlock)
    }

    /// Cover every window: drop the key (Unlocked -> Locked) and set the screen
    /// lock (`US-0159`). Does nothing without a vault. While an operation runs
    /// the request is remembered, as [`Self::lock`] does, so the operation's
    /// result never puts a key back or lifts the screen lock.
    pub fn lock_screen(&mut self, cx: &mut Context<Self>) {
        if !self.can_lock_screen() {
            return;
        }
        if self.busy {
            self.locked_meanwhile = true;
        }
        if let Phase::Unlocked(vault) = &self.phase {
            self.phase = Phase::Locked(vault.header().clone());
            cx.emit(VaultEvent::Locked);
        }
        if !self.screen_locked {
            self.screen_locked = true;
            cx.emit(VaultEvent::ScreenLocked);
        }
        cx.notify();
    }

    /// Lift the screen lock with `password`, verified against the header held
    /// in memory (never a fresh read of `vault.json`, so replacing the file
    /// cannot open a locked window): the screen opens and the vault is
    /// Unlocked. Wrong passwords count toward the dialogs' back-off.
    pub fn unlock_screen(
        &mut self,
        password: SecretString,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), VaultActionError>> {
        let Phase::Locked(header) = &self.phase else {
            return Task::ready(Err(VaultActionError::WrongPhase));
        };
        if !self.screen_locked {
            return Task::ready(Err(VaultActionError::WrongPhase));
        }
        let header = header.clone();
        if let Err(error) = self.check_attempt() {
            return Task::ready(Err(error));
        }
        self.run(
            cx,
            unlock_work(header, password),
            Self::finish_unlock_screen,
        )
    }

    /// Change the master password (Locked or Unlocked -> Unlocked; stays
    /// Locked when a lock was asked for while it ran). `old` is checked against
    /// the header held in memory.
    pub fn change_password(
        &mut self,
        old: SecretString,
        new: SecretString,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), VaultActionError>> {
        if let Err(error) = self.check_write(self.can_change()) {
            return Task::ready(Err(error));
        }
        if let Err(error) = self.check_attempt() {
            return Task::ready(Err(error));
        }
        let Some(header) = self.header().cloned() else {
            return Task::ready(Err(VaultActionError::WrongPhase));
        };
        let dir = self.dir.clone();
        self.run(
            cx,
            async move { rewrap(&dir, &header, &old, &new) },
            Self::finish_change,
        )
    }

    /// Drop the key: Unlocked -> Locked. While an operation runs, the request
    /// is also remembered, and the operation's result never puts a key back
    /// (a lock taken meanwhile wins, whatever the phase was).
    pub fn lock(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            self.locked_meanwhile = true;
        }
        let Phase::Unlocked(vault) = &self.phase else {
            return;
        };
        // Replacing the phase drops the `UnlockedVault`, which clears the key.
        self.phase = Phase::Locked(vault.header().clone());
        cx.emit(VaultEvent::Locked);
        cx.notify();
    }

    /// Remove `vault.json` and its backup -> Uninitialised. [`VaultEvent::Reset`]
    /// is emitted first so owners wipe their sealed values; the key is dropped
    /// at once. If the removal then fails the state stays Locked: owners must
    /// wipe idempotently and not assume Uninitialised follows `Reset`.
    pub fn reset(&mut self, cx: &mut Context<Self>) -> Task<Result<(), VaultActionError>> {
        if let Err(error) = self.check_write(self.can_reset()) {
            return Task::ready(Err(error));
        }
        if let Phase::Unlocked(vault) = &self.phase {
            self.phase = Phase::Locked(vault.header().clone());
        }
        cx.emit(VaultEvent::Reset);
        let dir = self.dir.clone();
        self.run(
            cx,
            async move { oneterm_vault::reset(&dir) },
            Self::finish_reset,
        )
    }

    fn header(&self) -> Option<&VaultHeader> {
        match &self.phase {
            Phase::Locked(header) => Some(header),
            Phase::Unlocked(vault) => Some(vault.header()),
            _ => None,
        }
    }

    fn check_write(&self, applies: bool) -> Result<(), VaultActionError> {
        if self.read_only {
            return Err(VaultActionError::Vault(VaultError::ReadOnly));
        }
        if self.busy {
            return Err(VaultActionError::Busy);
        }
        if !applies || self.screen_locked {
            return Err(VaultActionError::WrongPhase);
        }
        Ok(())
    }

    fn check_attempt(&self) -> Result<(), VaultActionError> {
        if self.busy {
            return Err(VaultActionError::Busy);
        }
        match self.retry_after() {
            Some(wait) => Err(VaultActionError::RetryIn(wait)),
            None => Ok(()),
        }
    }

    /// Mark the state busy, run `work` on the background executor, and apply
    /// its result with `apply` in a step this state detaches itself, so `busy`
    /// is always cleared even when the caller drops the returned task (which
    /// then only loses the answer).
    fn run<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        work: impl Future<Output = T> + Send + 'static,
        apply: impl FnOnce(&mut Self, T, &mut Context<Self>) -> Result<(), VaultActionError> + 'static,
    ) -> Task<Result<(), VaultActionError>> {
        self.begin();
        cx.notify();
        let work = cx.background_spawn(work);
        let (answer, receiver) = oneshot::channel();
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let outcome = this
                .update(cx, |state, cx| apply(state, result, cx))
                .unwrap_or(Err(VaultActionError::WrongPhase));
            // Nobody is listening when the caller dropped its task; the state
            // has been updated either way.
            _ = answer.send(outcome);
        })
        .detach();
        cx.spawn(async move |_, _| receiver.await.unwrap_or(Err(VaultActionError::WrongPhase)))
    }

    fn begin(&mut self) {
        self.busy = true;
        self.locked_meanwhile = false;
    }

    /// End the running operation; true when a lock was asked for while it ran.
    fn end(&mut self, cx: &mut Context<Self>) -> bool {
        self.busy = false;
        cx.notify();
        std::mem::take(&mut self.locked_meanwhile)
    }

    fn record_attempt<T>(&mut self, result: &Result<T, VaultActionError>) {
        match result {
            Ok(_) => {
                self.failures = 0;
                self.retry_at = None;
            }
            Err(error) if error.is_wrong_password() => {
                self.failures = self.failures.saturating_add(1);
                self.retry_at = backoff_after(self.failures).map(|wait| Instant::now() + wait);
            }
            Err(_) => {}
        }
    }

    fn finish_set_up(
        &mut self,
        result: Result<UnlockedVault, VaultError>,
        cx: &mut Context<Self>,
    ) -> Result<(), VaultActionError> {
        let locked_meanwhile = self.end(cx);
        match result {
            Ok(vault) => {
                self.failures = 0;
                self.retry_at = None;
                if locked_meanwhile {
                    // The vault exists now, but the lock wins: no key is kept.
                    self.phase = Phase::Locked(vault.header().clone());
                    cx.emit(VaultEvent::Initialised);
                    cx.emit(VaultEvent::Locked);
                } else {
                    self.phase = Phase::Unlocked(Box::new(vault));
                    cx.emit(VaultEvent::Initialised);
                }
                Ok(())
            }
            Err(error) => {
                if matches!(error, VaultError::AlreadyInitialised) {
                    // Another process was first. Leaving Uninitialised for what
                    // the file says only makes this process stricter.
                    self.phase = read_phase(&self.dir);
                }
                Err(VaultActionError::Vault(error))
            }
        }
    }

    fn finish_unlock(
        &mut self,
        result: Result<(UnlockedVault, SecretString), VaultError>,
        cx: &mut Context<Self>,
    ) -> Result<(), VaultActionError> {
        let locked_meanwhile = self.end(cx);
        if !matches!(self.phase, Phase::Locked(_)) {
            return Err(VaultActionError::WrongPhase);
        }
        let result = result.map_err(VaultActionError::Vault);
        self.record_attempt(&result);
        let (vault, password) = result?;
        if locked_meanwhile {
            // The password was right, but a lock asked for since wins: the key
            // is dropped here and the header stays as it was.
            return Err(VaultActionError::LockedMeanwhile);
        }
        self.keep_unlocked(vault, password, cx);
        Ok(())
    }

    fn finish_unlock_screen(
        &mut self,
        result: Result<(UnlockedVault, SecretString), VaultError>,
        cx: &mut Context<Self>,
    ) -> Result<(), VaultActionError> {
        let locked_meanwhile = self.end(cx);
        if !self.is_screen_locked() {
            return Err(VaultActionError::WrongPhase);
        }
        let result = result.map_err(VaultActionError::Vault);
        self.record_attempt(&result);
        let (vault, password) = result?;
        if locked_meanwhile {
            return Err(VaultActionError::LockedMeanwhile);
        }
        self.screen_locked = false;
        self.keep_unlocked(vault, password, cx);
        Ok(())
    }

    /// Hold `vault`'s key (-> Unlocked); a header below the default parameters
    /// is then rehashed with the password just typed.
    fn keep_unlocked(
        &mut self,
        vault: UnlockedVault,
        password: SecretString,
        cx: &mut Context<Self>,
    ) {
        let needs_rehash = vault.needs_rehash();
        self.phase = Phase::Unlocked(Box::new(vault));
        cx.emit(VaultEvent::Unlocked);
        if needs_rehash && !self.read_only {
            self.rehash(password, cx);
        }
    }

    fn finish_change(
        &mut self,
        result: Result<UnlockedVault, VaultError>,
        cx: &mut Context<Self>,
    ) -> Result<(), VaultActionError> {
        let locked_meanwhile = self.end(cx);
        let result = result.map_err(VaultActionError::Vault);
        self.record_attempt(&result);
        let vault = result?;
        if locked_meanwhile {
            // `vault.json` now holds the new header; keep only that.
            self.phase = Phase::Locked(vault.header().clone());
            return Ok(());
        }
        let was_locked = matches!(self.phase, Phase::Locked(_));
        self.phase = Phase::Unlocked(Box::new(vault));
        if was_locked {
            cx.emit(VaultEvent::Unlocked);
        }
        Ok(())
    }

    fn finish_reset(
        &mut self,
        result: Result<(), VaultError>,
        cx: &mut Context<Self>,
    ) -> Result<(), VaultActionError> {
        // The key was dropped when the reset started; a lock meanwhile has
        // nothing more to take.
        self.end(cx);
        result.map_err(VaultActionError::Vault)?;
        self.phase = Phase::Uninitialised;
        // No vault, no lock: a reset is refused while the screen is locked, but
        // one started just before a lock still lands here.
        self.screen_locked = false;
        self.failures = 0;
        self.retry_at = None;
        Ok(())
    }

    /// Rewrap the header with the default parameters, keeping `password` alive
    /// until the background run finishes (`US-0156` carry-forward).
    fn rehash(&mut self, password: SecretString, cx: &mut Context<Self>) {
        let Some(header) = self.header().cloned() else {
            return;
        };
        let dir = self.dir.clone();
        // Nobody awaits a rehash; `run` applies it regardless.
        self.run(
            cx,
            async move { rewrap(&dir, &header, &password, &password) },
            Self::finish_rehash,
        )
        .detach();
    }

    fn finish_rehash(
        &mut self,
        result: Result<UnlockedVault, VaultError>,
        cx: &mut Context<Self>,
    ) -> Result<(), VaultActionError> {
        let locked_meanwhile = self.end(cx);
        match result {
            // `vault.json` now holds the new header. Only Lock can have run
            // meanwhile (`busy` holds the rest off), and it wins: a locked
            // process keeps just the header.
            Ok(vault) => match self.phase {
                Phase::Unlocked(_) if !locked_meanwhile => {
                    self.phase = Phase::Unlocked(Box::new(vault));
                }
                Phase::Locked(_) | Phase::Unlocked(_) => {
                    self.phase = Phase::Locked(vault.header().clone());
                }
                Phase::Uninitialised | Phase::Unavailable { .. } => {}
            },
            Err(error) if is_expected_rehash_refusal(&error) => {
                log::info!("vault rehash skipped: {error}");
            }
            Err(error) => log::warn!("vault rehash failed: {error}"),
        }
        Ok(())
    }
}

/// What `dir/vault.json` says, as a phase. Never writes, never quarantines.
fn read_phase(dir: &Path) -> Phase {
    match oneterm_vault::load(dir) {
        Ok(VaultStatus::Uninitialised) => Phase::Uninitialised,
        Ok(VaultStatus::Ready(header)) => Phase::Locked(header),
        Ok(VaultStatus::Damaged(reason)) => {
            log::warn!("vault.json is damaged and left in place: {reason}");
            Phase::Unavailable {
                reason: format!("vault.json is damaged ({reason}). Reset is the only way back."),
                resettable: true,
            }
        }
        Ok(VaultStatus::Unsupported(reason)) => Phase::Unavailable {
            reason: format!("vault.json was created by a newer OneTerm ({reason})."),
            resettable: false,
        },
        Err(error) => {
            log::warn!("vault.json could not be read: {error}");
            Phase::Unavailable {
                reason: format!("{error}."),
                resettable: false,
            }
        }
    }
}

/// One Argon2id run against `header`. The password goes back with the vault:
/// the rehash after an unlock needs it.
async fn unlock_work(
    header: VaultHeader,
    password: SecretString,
) -> Result<(UnlockedVault, SecretString), VaultError> {
    oneterm_vault::unlock(&header, &password).map(|vault| (vault, password))
}

/// Set up a vault in `dir`. Tests derive at `KdfParams::FLOOR`.
fn create_vault(dir: &Path, password: &SecretString) -> Result<UnlockedVault, VaultError> {
    #[cfg(not(test))]
    {
        oneterm_vault::init(dir, password)
    }
    #[cfg(test)]
    {
        oneterm_vault::init_with_params(dir, password, oneterm_vault::KdfParams::FLOOR)
    }
}

/// Unlock `header` with `old` and rewrap it under `new`, on a second instance:
/// the state's own `UnlockedVault` stays in place until this one succeeds.
/// Costs one Argon2id run more than `change_password` alone.
fn rewrap(
    dir: &Path,
    header: &VaultHeader,
    old: &SecretString,
    new: &SecretString,
) -> Result<UnlockedVault, VaultError> {
    let mut vault = oneterm_vault::unlock(header, old)?;
    #[cfg(not(test))]
    vault.change_password(dir, old, new)?;
    #[cfg(test)]
    vault.change_password_with_params(dir, old, new, oneterm_vault::KdfParams::FLOOR)?;
    Ok(vault)
}

#[cfg(test)]
#[path = "vault_state_tests.rs"]
mod vault_state_tests;
