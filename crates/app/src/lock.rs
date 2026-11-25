//! The lock (specs/oneterm, "Lock", "M6 in detail"): with a master password, the window locks
//! after a time idle or at once, and only the password opens it. Five wrong tries start a
//! back-off. The password is the one that opens the vault (`vault.rs`), tried on the pool.

use std::time::Duration;

use quadra::{Scope, Signal, background, interval, signal, spawn, untrack};

use crate::prefs::Prefs;

/// Idle is counted in these steps.
const STEP: u32 = 10;
/// The waits, in seconds, after the fifth wrong try and each one after it.
const BACK_OFF: [u32; 5] = [2, 4, 8, 16, 30];

#[derive(Clone, Copy)]
pub struct Lock {
    pub locked: Signal<bool>,
    /// The minutes idle it locked after; 0 when it was locked at once.
    pub after: Signal<u32>,
    /// What the password field holds.
    pub typed: Signal<String>,
    /// What the card says under the field: a wrong password, the wait.
    pub said: Signal<String>,
    /// Seconds left before another try.
    pub wait: Signal<u32>,
    /// The saved passwords' data key, once the master password was typed (`vault.rs`).
    pub key: Signal<Option<crate::vault::DataKey>>,
    tries: Signal<u32>,
    idle: Signal<u32>,
    prefs: Prefs,
    owner: Scope,
}

impl Lock {
    /// The lock of a window: its timers live in the scope it is made in.
    pub(crate) fn new(prefs: Prefs) -> Lock {
        let lock = Lock {
            locked: signal(false),
            after: signal(0),
            typed: signal(String::new()),
            said: signal(String::new()),
            wait: signal(0),
            key: signal(None),
            tries: signal(0),
            idle: signal(0),
            prefs,
            owner: Scope::current(),
        };
        // With a vault the window opens locked (specs/oneterm, "M6 in detail").
        if untrack(|| lock.can_lock()) {
            lock.locked.set(true);
        }
        let _ = interval(Duration::from_secs(u64::from(STEP)), move || lock.tick());
        let _ = interval(Duration::from_secs(1), move || {
            if lock.wait.get_untracked() > 0 {
                lock.wait.update(|n| *n -= 1);
            }
        });
        lock
    }

    /// Whether there is a master password (a vault it opens), so the window can lock.
    pub fn can_lock(&self) -> bool {
        !self.prefs.vault.with(String::is_empty)
    }

    /// How many hosts have a secret saved under the master password.
    pub fn saved_secrets(&self) -> usize {
        self.prefs.secrets.with_untracked(Vec::len)
    }

    /// Whole minutes left before the window locks while idle; None when it cannot lock.
    pub fn left(&self) -> Option<u32> {
        if !self.can_lock() {
            return None;
        }
        let minutes = self.prefs.lock_after.get().clamp(1.0, 480.0) as u32;
        Some((minutes * 60).saturating_sub(self.idle.get()).div_ceil(60))
    }

    /// Something was done in the window: idle starts again.
    pub fn active(&self) {
        if self.idle.get_untracked() != 0 {
            self.idle.set(0);
        }
    }

    pub fn lock_now(&self) {
        self.close(0);
    }

    fn close(&self, after: u32) {
        if untrack(|| self.can_lock()) && !self.locked.get_untracked() {
            self.after.set(after);
            self.locked.set(true);
        }
    }

    fn tick(&self) {
        let idle = self.idle.get_untracked() + STEP;
        self.idle.set(idle);
        let minutes = self.prefs.lock_after.get_untracked().clamp(1.0, 480.0) as u32;
        if idle >= minutes * 60 {
            self.close(minutes);
        }
    }

    /// Tries what the field holds: right, the window opens; wrong, it is said, and from the
    /// fifth wrong try the next waits.
    pub fn unlock(&self) {
        if self.wait.get_untracked() > 0 {
            return;
        }
        let lock = *self;
        self.open(self.typed.get_untracked(), move |right| {
            crate::vault::wipe(lock.typed);
            if right {
                lock.tries.set(0);
                lock.said.set(String::new());
                lock.idle.set(0);
                lock.locked.set(false);
                return;
            }
            let tries = lock.tries.get_untracked() + 1;
            lock.tries.set(tries);
            let wait = tries.checked_sub(5).map(|n| BACK_OFF[(n as usize).min(4)]);
            lock.wait.set(wait.unwrap_or(0));
            lock.said.set(match wait {
                Some(s) => format!("Wrong password \u{B7} try again in {s} s"),
                None => "Wrong password".into(),
            });
        });
    }

    /// Tries the master password on the pool: right, the saved passwords' data key is in
    /// memory. `done` is told whether it was right.
    pub fn open(&self, typed: String, done: impl FnOnce(bool) + 'static) {
        let typed = quadra_shell::Zeroizing::new(typed);
        let (key, sealed) = (self.key, self.prefs.vault.get_untracked());
        self.owner.run(|| {
            spawn(async move {
                let opened = background(move || crate::vault::unseal(&typed, &sealed)).await;
                let Ok(Some(data)) = opened else { return done(false) };
                key.set(Some(std::rc::Rc::new(data)));
                done(true);
            })
            .detach()
        });
    }

    /// Sets, changes or removes (an empty `new`) the master password; `current` must be the
    /// one set, if any. `done` is told what came of it.
    pub fn set_password(&self, current: String, new: String, done: impl Fn(&str) + 'static) {
        let (current, new) = (quadra_shell::Zeroizing::new(current), quadra_shell::Zeroizing::new(new));
        let (sealed, (vault, secrets, key)) =
            (self.prefs.vault.get_untracked(), (self.prefs.vault, self.prefs.secrets, self.key));
        self.owner.run(|| {
            spawn(async move {
                let made = background(move || {
                    let opened = crate::vault::unseal(&current, &sealed);
                    if !sealed.is_empty() && opened.is_none() {
                        return Err("The current password is wrong");
                    }
                    if new.is_empty() {
                        return Ok((String::new(), None));
                    }
                    // The same vault and data key under the new password, so what it sealed
                    // still opens; the first password makes the vault.
                    let made = match opened {
                        Some(data) => crate::vault::reseal(&data, &new).map(|vault| (vault, data)),
                        None => crate::vault::make(&new),
                    };
                    made.map(|(vault, data)| (vault, Some(data))).ok_or("Sealing failed")
                })
                .await;
                match made {
                    Ok(Ok((sealed, data))) => {
                        let removed = sealed.is_empty();
                        vault.set(sealed);
                        key.set(data.map(std::rc::Rc::new));
                        // Removed, the saved secrets go with it.
                        if removed {
                            secrets.set(Vec::new());
                        }
                        done(if removed { "Password removed" } else { "Password set" });
                    }
                    Ok(Err(why)) => done(why),
                    Err(_) => done("Sealing failed"),
                }
            })
            .detach()
        });
    }
}
