//! The Connect dialog's Username: `user`, or `user@host:port`.

use oneterm_ssh::Target;

/// `user`, or `user@host:port`: what it gives takes the place of the target's.
pub(super) fn overridden(target: &Target, typed: &str) -> Target {
    let mut target = target.clone();
    let (user, place) = match typed.trim().rsplit_once('@') {
        Some((user, place)) => (user, place),
        None => (typed.trim(), ""),
    };
    if !user.is_empty() {
        target.user = user.to_string();
    }
    let (host, port) = match place.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') || host.ends_with(']') => match port.parse::<u16>() {
            Ok(port) => (host, Some(port)),
            Err(_) => (place, None),
        },
        _ => (place, None),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if !host.is_empty() {
        target.host = host.to_string();
    }
    target.port = port.unwrap_or(target.port);
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_username_gives_takes_the_saved_target_s_place() {
        let saved = Target { user: "trung".into(), host: "10.0.0.9".into(), port: 22 };
        let read = |typed: &str| overridden(&saved, typed).label();
        assert_eq!(read(""), "trung@10.0.0.9");
        assert_eq!(read("root"), "root@10.0.0.9");
        assert_eq!(read("root@db"), "root@db");
        assert_eq!(read("root@db:2222"), "root@db:2222");
        assert_eq!(read("@:2200"), "trung@10.0.0.9:2200");
        assert_eq!(read("ops@[::1]:2022"), overridden(&saved, "ops@[::1]:2022").label());
        assert_eq!(overridden(&saved, "ops@[::1]:2022").host, "::1");
    }
}
