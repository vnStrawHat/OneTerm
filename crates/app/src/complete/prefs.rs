//! The settings of completion (specs/oneterm, "Completion"): signals of the app's `Settings` under
//! `completion.*`, and the engine's `Config` they make.

use std::rc::Rc;

use oneterm_completion::{Config, Family, Sources};
use quadra::{Settings, Signal, use_context};

/// The choices of the shell family; the first is the one detected.
pub const FAMILIES: [&str; 4] = ["Detected", "Cmd", "PowerShell", "Unix"];

/// The settings, as signals: writing one saves it.
#[derive(Clone, Copy)]
pub struct CompletePrefs {
    pub enabled: Signal<bool>,
    pub accept_tab: Signal<bool>,
    pub max_history: Signal<f64>,
    pub min_prefix: Signal<f64>,
    pub max_visible: Signal<f64>,
    /// The sources: the lines run this session, the catalogs written by hand, the generated ones.
    pub memory: Signal<bool>,
    pub manual: Signal<bool>,
    pub external: Signal<bool>,
    pub fuzzy: Signal<bool>,
    pub ancestors: Signal<bool>,
    pub off_in_alt_screen: Signal<bool>,
    pub only_at_prompt: Signal<bool>,
    pub coreutils: Signal<bool>,
    /// An index of `FAMILIES`.
    pub family: Signal<usize>,
    pub redact: Signal<bool>,
}

impl CompletePrefs {
    /// The app's settings, from the context; in memory where there are none (a test).
    pub fn load() -> CompletePrefs {
        CompletePrefs::of(&use_context::<Rc<Settings>>().unwrap_or_else(Settings::memory))
    }

    /// What each setting is where nothing was written: what Reset puts back.
    pub fn defaults() -> CompletePrefs {
        CompletePrefs::of(&Settings::memory())
    }

    fn of(s: &Settings) -> CompletePrefs {
        let on = Config::default();
        let count = |key: &str, default: usize| s.value(key, default as f64);
        CompletePrefs {
            // Off until asked for: the app, not the engine, decides that.
            enabled: s.value("completion.enabled", false),
            accept_tab: s.value("completion.accept_tab", on.accept_tab),
            max_history: count("completion.max_history", on.max_history),
            min_prefix: count("completion.min_prefix_len", on.min_prefix_len),
            max_visible: count("completion.max_visible_items", on.max_visible_items),
            memory: s.value("completion.sources.memory", on.sources.memory),
            manual: s.value("completion.sources.manual", on.sources.manual),
            external: s.value("completion.sources.external", on.sources.external),
            fuzzy: s.value("completion.fuzzy", on.fuzzy),
            ancestors: s.value("completion.inherit_ancestor_options", on.inherit_ancestor_options),
            off_in_alt_screen: s.value("completion.disable_in_alt_screen", on.disable_in_alt_screen),
            only_at_prompt: s.value("completion.require_prompt_region", on.require_prompt_region),
            coreutils: s.value("completion.windows_allow_coreutils", on.windows_allow_coreutils),
            family: s.choice("completion.force_family", &FAMILIES),
            redact: s.value("completion.redact_sensitive", on.redact_sensitive),
        }
    }

    /// The engine's settings, read where it is called.
    pub fn config(&self) -> Config {
        let count = |value: Signal<f64>, most: f64| value.get().clamp(0.0, most) as usize;
        Config {
            enabled: self.enabled.get(),
            accept_tab: self.accept_tab.get(),
            max_history: count(self.max_history, 5000.0),
            min_prefix_len: count(self.min_prefix, 10.0),
            max_visible_items: count(self.max_visible, 20.0).max(1),
            sources: Sources {
                memory: self.memory.get(),
                manual: self.manual.get(),
                external: self.external.get(),
            },
            fuzzy: self.fuzzy.get(),
            inherit_ancestor_options: self.ancestors.get(),
            disable_in_alt_screen: self.off_in_alt_screen.get(),
            require_prompt_region: self.only_at_prompt.get(),
            windows_allow_coreutils: self.coreutils.get(),
            force_family: Family::parse(FAMILIES[self.family.get().min(FAMILIES.len() - 1)]),
            redact_sensitive: self.redact.get(),
        }
    }
}
