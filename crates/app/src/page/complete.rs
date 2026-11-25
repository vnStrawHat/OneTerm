//! The settings of command completion (specs/oneterm, "Completion"), a group of the Terminal page.

use quadra_design::{SettingsGroup, number_input, select, switch};

use super::Back;
use crate::complete::{CompletePrefs, FAMILIES};
use crate::model::Model;

pub(super) fn completion(model: &Model) -> SettingsGroup {
    let (c, d): (CompletePrefs, _) = (model.ui().complete.prefs, CompletePrefs::defaults());
    SettingsGroup::new("Completion")
        .row("Completion", "Suggestions under the cursor as a command is typed.", switch(c.enabled))
        .back(c.enabled, d.enabled)
        .row(
            "Accept with Tab",
            "Tab chooses a suggestion, then takes it; off, Tab goes to the shell.",
            switch(c.accept_tab),
        )
        .back(c.accept_tab, d.accept_tab)
        .row(
            "History kept",
            "Commands remembered for each kind of shell. 0 remembers none.",
            number_input(c.max_history).stepper().range(0.0, 5000.0).step(50.0).label("History kept"),
        )
        .back(c.max_history, d.max_history)
        .row(
            "Characters before suggesting",
            "Characters typed before suggestions appear.",
            number_input(c.min_prefix).stepper().range(0.0, 10.0).label("Characters before suggesting"),
        )
        .back(c.min_prefix, d.min_prefix)
        .row(
            "Rows shown",
            "Suggestions shown before the list scrolls.",
            number_input(c.max_visible).stepper().range(1.0, 20.0).label("Rows shown"),
        )
        .back(c.max_visible, d.max_visible)
        .row("From the history", "The commands run this session.", switch(c.memory))
        .back(c.memory, d.memory)
        .row(
            "From hand-written catalogs",
            "Commands and their options written for OneTerm.",
            switch(c.manual),
        )
        .back(c.manual, d.manual)
        .row("From generated catalogs", "Commands read from the tools' own help.", switch(c.external))
        .back(c.external, d.external)
        .row("Fuzzy matching", "Also match names that have the typed letters in order.", switch(c.fuzzy))
        .back(c.fuzzy, d.fuzzy)
        .row(
            "Options of the commands above",
            "A subcommand also offers its command's options.",
            switch(c.ancestors),
        )
        .back(c.ancestors, d.ancestors)
        .row(
            "Off in full-screen programs",
            "Nothing on the alternate screen: vim, less, htop.",
            switch(c.off_in_alt_screen),
        )
        .back(c.off_in_alt_screen, d.off_in_alt_screen)
        .row("Only at a prompt", "Suggest only where a command is being typed.", switch(c.only_at_prompt))
        .back(c.only_at_prompt, d.only_at_prompt)
        .row("Unix commands on Windows", "Cmd and PowerShell also offer coreutils.", switch(c.coreutils))
        .back(c.coreutils, d.coreutils)
        .row(
            "Shell family",
            "The kind of shell completed for; detected from the shell.",
            select(FAMILIES, c.family).width(240.0),
        )
        .back(c.family, d.family)
        .row(
            "Redact secrets",
            "Passwords and tokens are taken out of a line before it is kept.",
            switch(c.redact),
        )
        .back(c.redact, d.redact)
}
