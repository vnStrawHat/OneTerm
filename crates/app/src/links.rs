//! Links in a space's terminal (specs/oneterm, "Spaces"): Ctrl+click, or the menu's Open link,
//! goes through a policy of the target and its visible text: opened by the shell's opener,
//! asked about first in a dialog, or refused with a card saying why.

use std::rc::Rc;

use quadra::{Clipboard, Signal, View, signal, text, use_context};
use quadra_design::{MenuItem, Notice, Tone, Variant, dialog};
use quadra_shell::Opener;
use quadra_term::TermLink;

use crate::model::Model;

/// What the policy makes of a link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Open,
    Ask,
    /// Why not, as the end of a sentence.
    Refuse(&'static str),
}

/// The longest target looked at.
const MOST: usize = 256 * 1024;

/// `https` opens; `http`, a port not the scheme's own, or visible text that names another
/// host asks first; any other scheme, no host, a control, space or direction character, a user or
/// password, or a target over 256 KiB is refused.
pub(crate) fn verdict(link: &TermLink) -> Verdict {
    let url = link.url.as_str();
    if url.len() > MOST {
        return Verdict::Refuse("it is longer than 256 KiB");
    }
    let turns =
        |c: char| matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}');
    if url.contains(|c: char| c.is_control() || c.is_whitespace() || turns(c)) {
        return Verdict::Refuse("it has a control, space or direction character");
    }
    let (scheme, rest) = url.split_once(':').unwrap_or_default();
    let own_port = match scheme.to_ascii_lowercase().as_str() {
        "https" => "443",
        "http" => "80",
        _ => return Verdict::Refuse("only web links open"),
    };
    let after = rest.strip_prefix("//").unwrap_or_default();
    let authority = &after[..after.find(['/', '\\', '?', '#']).unwrap_or(after.len())];
    if authority.is_empty() {
        return Verdict::Refuse("it names no host");
    }
    if authority.contains('@') {
        return Verdict::Refuse("it carries a user or a password");
    }
    let port = authority.rsplit_once(']').map_or(authority, |it| it.1).rsplit_once(':').map(|it| it.1);
    let other_port = port.is_some_and(|it| !it.is_empty() && it != own_port);
    let misnamed = link.text.as_deref().and_then(host).is_some_and(|shown| host(url) != Some(shown));
    if own_port == "80" || other_port || misnamed { Verdict::Ask } else { Verdict::Open }
}

/// The host of text that looks like an address (after a scheme's `//`, or a first word with a
/// dot), lower case, without `www.`, a user or the port; none for words like "click here".
fn host(text: &str) -> Option<String> {
    let text = text.trim();
    let (rest, schemed) = text.split_once("://").map_or((text, false), |it| (it.1, true));
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let at = authority.rsplit('@').next().unwrap_or(authority);
    let host = match at.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(v6),
        None => {
            at.rsplit_once(':').filter(|it| it.1.bytes().all(|b| b.is_ascii_digit())).map_or(at, |it| it.0)
        }
    };
    let host = host.trim_end_matches('.').to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let odd = |c: char| c.is_whitespace() || matches!(c, '\\' | '"' | '\'');
    (!host.is_empty() && (schemed || host.contains('.')) && !host.contains(odd)).then(|| host.to_string())
}

/// A link waiting for its answer, and whether its dialog shows.
#[derive(Clone, Copy)]
pub(crate) struct Asking {
    open: Signal<bool>,
    link: Signal<Option<TermLink>>,
}

/// What following a link needs from where the space was built.
#[derive(Clone)]
pub(crate) struct Links {
    model: Model,
    asking: Asking,
    opener: Option<Rc<dyn Opener>>,
    clipboard: Option<Rc<dyn Clipboard>>,
}

impl Links {
    pub fn here(model: &Model) -> Links {
        let asking = Asking { open: signal(false), link: signal(None) };
        let (opener, clipboard) = (use_context::<Rc<dyn Opener>>(), use_context::<Rc<dyn Clipboard>>());
        Links { model: model.clone(), asking, opener, clipboard }
    }

    /// Opens, asks or refuses, as the policy says.
    pub fn follow(&self, link: TermLink) {
        match verdict(&link) {
            Verdict::Open => self.open(&link.url),
            Verdict::Ask => {
                self.asking.link.set(Some(link));
                self.asking.open.set(true);
            }
            Verdict::Refuse(why) => {
                let body = format!("{} was not opened: {why}.", shown(&link.url));
                self.model.notify(Notice::new("Link").body(body).tone(Tone::Warning));
            }
        }
    }

    fn open(&self, url: &str) {
        if let Some(opener) = &self.opener {
            opener.open(url);
        }
    }

    /// The dialog that asks: the target, and the visible text when it differs.
    pub fn dialog(&self) -> impl View + use<> {
        let (asking, opens) = (self.asking, self.clone());
        let says = move || asking.link.with(|it| it.as_ref().map(question).unwrap_or_default());
        dialog(asking.open, "Open link?", move || text(says))
            .width(360.0)
            .action("Cancel", Variant::Secondary, || {})
            .action("Open", Variant::Primary, move || {
                if let Some(link) = asking.link.get_untracked() {
                    opens.open(&link.url);
                }
            })
    }

    /// The menu's items for a link the other button was pressed on.
    pub fn menu(&self, link: TermLink) -> Vec<MenuItem> {
        let (follows, url, clipboard) = (self.clone(), link.url.clone(), self.clipboard.clone());
        vec![
            MenuItem::new("Open link").on_select(move || follows.follow(link.clone())),
            MenuItem::new("Copy link").on_select(move || {
                if let Some(clipboard) = &clipboard {
                    clipboard.write_text(&url);
                }
            }),
            MenuItem::separator(),
        ]
    }
}

fn question(link: &TermLink) -> String {
    match &link.text {
        Some(label) => {
            format!("This link shows \"{}\" but opens {}. Open it?", shown(label), shown(&link.url))
        }
        None => format!("Open {} in the browser?", shown(&link.url)),
    }
}

/// At most 300 characters of what a program wrote, controls shown as such.
fn shown(text: &str) -> String {
    let mut out: String =
        text.chars().take(300).map(|c| if c.is_control() { '\u{FFFD}' } else { c }).collect();
    if text.chars().nth(300).is_some() {
        out.push('\u{2026}');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(url: &str, text: Option<&str>) -> Verdict {
        verdict(&TermLink { url: url.into(), text: text.map(Into::into) })
    }

    #[test]
    fn https_opens_http_asks_and_the_rest_is_refused() {
        for url in ["https://example.com/a", "HTTPS://Example.com", "https://a.b:443/x", "https://[::1]/"] {
            assert_eq!(of(url, None), Verdict::Open, "{url}");
        }
        for url in ["http://example.com", "https://a.b:8443/", "https://[::1]:8443/", "https://a.b:0443/"] {
            assert_eq!(of(url, None), Verdict::Ask, "{url}");
        }
        assert_eq!(of("https://a.b:/", None), Verdict::Open, "an empty port is the scheme's");
        for url in [
            "javascript:alert(1)",
            "file:///C:/Windows/System32/cmd.exe",
            "custom-app://run",
            "not a url",
            "https://user:secret@example.com/",
            "http://user@example.com/",
            "https://a.b/\u{7}x",
            "https://a.b/a\nb",
            "https://a.b/\u{202e}gnp",
            "https://a.b/\u{9b}",
            "https:/u:p@evil.test/",
            "https:\\\\u:p@evil.test",
            "https:///u:p@evil.test",
            "https://\\u@evil.test",
            "https:evil.test:8443",
            "https://",
        ] {
            assert!(matches!(of(url, None), Verdict::Refuse(_)), "{url:?}");
        }
        let long = format!("https://a.b/{}", "x".repeat(MOST));
        assert_eq!(of(&long, None), Verdict::Refuse("it is longer than 256 KiB"));
    }

    #[test]
    fn visible_text_that_names_another_host_asks() {
        for text in [
            "click here",
            "Release notes 1.2",
            "example.com",
            "WWW.Example.com/x",
            "https://example.com:443/",
        ] {
            assert_eq!(of("https://example.com/y", Some(text)), Verdict::Open, "{text}");
        }
        for text in ["https://good.test", "good.test/login", "www.good.test"] {
            assert_eq!(of("https://evil.test/login", Some(text)), Verdict::Ask, "{text}");
        }
        assert!(
            matches!(of("javascript:x", Some("https://a.b")), Verdict::Refuse(_)),
            "refused stays refused"
        );
    }
}
