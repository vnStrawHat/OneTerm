//! Where to connect (specs/oneterm, "M7 in detail"): read from what a user types.

/// A user at a host and port; the user is empty when none was given (connecting asks).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub user: String,
    pub host: String,
    pub port: u16,
}

impl Target {
    /// `user@host`, `host:port`, `[user@]host -p port`, `-l user`, a leading `ssh`; no user is
    /// empty, no port 22. None when there is no host or a part does not read.
    pub fn parse(text: &str) -> Option<Target> {
        let mut words = text.split_whitespace().peekable();
        if words.peek() == Some(&"ssh") {
            words.next();
        }
        let (mut user, mut host, mut port) = (None, None, None);
        while let Some(word) = words.next() {
            match word {
                "-p" => port = Some(words.next()?.parse().ok()?),
                "-l" => user = Some(words.next()?.to_string()),
                _ if word.starts_with('-') => return None,
                _ if host.is_none() => host = Some(word),
                _ => return None,
            }
        }
        let mut host = host?;
        if let Some((name, at)) = host.rsplit_once('@') {
            user = user.or(Some(name.to_string()));
            host = at;
        }
        // `host:port`, or `[v6]:port`; a bare v6 address has more than one colon.
        let host = match host.rsplit_once(':') {
            Some((name, number)) if !name.contains(':') || name.starts_with('[') => {
                port = port.or(Some(number.parse().ok()?));
                name.trim_start_matches('[').trim_end_matches(']')
            }
            _ => host,
        };
        if host.is_empty() || user.as_deref() == Some("") {
            return None;
        }
        Some(Target { user: user.unwrap_or_default(), host: host.to_string(), port: port.unwrap_or(22) })
    }

    /// `user@host` (`host` without a user), with `:port` when it is not 22.
    pub fn label(&self) -> String {
        let at = if self.user.is_empty() { String::new() } else { format!("{}@", self.user) };
        match self.port {
            22 => format!("{at}{}", self.host),
            port => format!("{at}{}:{port}", self.host),
        }
    }
}

/// The OS's user, what is filled in when none was given.
pub fn os_user() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "root".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(user: &str, host: &str, port: u16) -> Option<Target> {
        Some(Target { user: user.into(), host: host.into(), port })
    }

    #[test]
    fn what_a_user_types_reads() {
        assert_eq!(Target::parse("trung@10.20.3.41 -p 2222"), t("trung", "10.20.3.41", 2222));
        assert_eq!(Target::parse("ssh a@h"), t("a", "h", 22));
        assert_eq!(Target::parse("a@h:2200"), t("a", "h", 2200));
        assert_eq!(Target::parse("-l bob host"), t("bob", "host", 22));
        assert_eq!(Target::parse("a@[::1]:22"), t("a", "::1", 22));
        assert_eq!(Target::parse("a@fe80::1"), t("a", "fe80::1", 22));
        assert_eq!(Target::parse("a@h").map(|it| it.label()), Some("a@h".into()));
        assert_eq!(Target::parse("a@h:99").map(|it| it.label()), Some("a@h:99".into()));
        assert_eq!(Target::parse("h:99"), t("", "h", 99));
        assert_eq!(Target::parse("h:99").map(|it| it.label()), Some("h:99".into()));
        for bad in ["", "ssh", "a@", "@h", "h -p", "h -p x", "h -x", "h extra"] {
            assert_eq!(Target::parse(bad), None, "{bad:?}");
        }
    }
}
