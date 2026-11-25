//! What OneTerm's process uses (specs/oneterm, "M4 in detail"): read every 2 s from the context's
//! `Meter` (the OS's where there is none), the status bar's CPU and RAM, and the tooltip of
//! details under the pointer.

use std::rc::Rc;
use std::time::Duration;

use quadra::{AnyView, Signal, View, div, interval, row, text, time, use_context};
use quadra_design::{description_list, fonts, token, tooltip};
use quadra_shell::{Meter, Usage};

/// The local time the status bar's clock shows: the system's, or a test's.
pub type Clock = Rc<dyn Fn() -> time::DateTime>;

/// A reading, and the processor's use since the one before, in percent of all processors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Reading {
    pub cpu: f64,
    pub usage: Usage,
}

const MB: f64 = 1024.0 * 1024.0;

fn os() -> Option<Usage> {
    #[cfg(windows)]
    return quadra_shell::process_usage();
    #[cfg(not(windows))]
    None
}

/// The readings, again every 2 s.
pub(crate) fn follow() -> Signal<Option<Reading>> {
    let (seen, meter) = (quadra::signal(None::<Reading>), use_context::<Rc<dyn Meter>>());
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
    let read = move || {
        let Some(usage) = meter.as_ref().map_or_else(os, |it| it.usage()) else { return };
        // The processor time between two readings over the time the process lived between them.
        let cpu = seen.get_untracked().map_or(0.0, |was: Reading| {
            let lived = usage.uptime.saturating_sub(was.usage.uptime).as_secs_f64();
            let used = usage.cpu.saturating_sub(was.usage.cpu).as_secs_f64();
            if lived > 0.0 { (used / lived / cores * 100.0).round() } else { was.cpu }
        });
        seen.set(Some(Reading { cpu, usage }));
    };
    read();
    let _ = interval(Duration::from_secs(2), read);
    seen
}

/// `3%`.
pub(crate) fn cpu(seen: Signal<Option<Reading>>) -> impl Fn() -> String + Clone + 'static {
    move || seen.with(|it| it.map(|r| format!("{}%", r.cpu)).unwrap_or_default())
}

/// `182 MB`: the private working set.
pub(crate) fn ram(seen: Signal<Option<Reading>>) -> impl Fn() -> String + Clone + 'static {
    move || seen.with(|it| it.map(|r| format!("{} MB", r.usage.memory / (1024 * 1024))).unwrap_or_default())
}

/// `1:02:03`.
fn hms(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// A value of the card, in mono, following the readings.
fn value(seen: Signal<Option<Reading>>, of: impl Fn(&Reading) -> String + 'static) -> AnyView {
    let words = move || seen.with(|it| it.as_ref().map(&of).unwrap_or_default());
    AnyView::new(div().font_family(fonts::MONO).text_color(token::TEXT).child(text(words)))
}

/// A column of the card: its heading, then terms and values.
fn group(title: &'static str, items: Vec<(&'static str, AnyView)>) -> quadra::Div {
    let heading = div().font_weight(600).text_color(token::TEXT_2).child(text(title));
    quadra_design::vstack().child(heading).child(description_list(items))
}

/// The details: memory | processor, following the readings.
fn details(seen: Signal<Option<Reading>>) -> quadra::Div {
    let mb = |f: fn(&Usage) -> u64| value(seen, move |r| format!("{:.1} MB", f(&r.usage) as f64 / MB));
    let memory = vec![
        ("Private working set", mb(|u| u.memory)),
        ("Working set", mb(|u| u.working_set)),
        ("Commit (private bytes)", mb(|u| u.private_bytes)),
        ("Peak working set", mb(|u| u.peak_working_set)),
    ];
    let processor = vec![
        ("CPU usage", value(seen, |r| format!("{}%", r.cpu))),
        ("CPU time", value(seen, |r| hms(r.usage.cpu))),
        ("Threads", value(seen, |r| r.usage.threads.to_string())),
        ("Uptime", value(seen, |r| hms(r.usage.uptime))),
    ];
    let line = div().width(1.0).shrink(0.0).bg(token::LINE);
    row()
        .gap(token::SPACE_XL)
        .text_size(12.0)
        .child(group("Memory", memory))
        .child(line)
        .child(group("Processor", processor))
}

/// `of` with the tooltip of details: memory | processor, laid out as a card; a reader is told
/// what OneTerm uses.
pub(crate) fn tip(of: quadra::Div, seen: Signal<Option<Reading>>) -> impl View + use<> {
    let said = move || {
        let mb = |r: &Reading| r.usage.memory / (1024 * 1024);
        seen.with(|it| it.map(|r| format!("OneTerm: CPU {}%, RAM {} MB", r.cpu, mb(&r))).unwrap_or_default())
    };
    tooltip(said, of).content(move || details(seen))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_duration_is_hours_minutes_and_seconds() {
        let d = |s| super::hms(std::time::Duration::from_secs(s));
        assert_eq!(
            (d(0), d(59), d(3723), d(90_000)),
            ("0:00:00".into(), "0:00:59".into(), "1:02:03".into(), "25:00:00".into())
        );
    }
}
