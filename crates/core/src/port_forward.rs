//! Port-forward specifications (IN-0023 / US-0059): persisted by the session
//! store, run by the SSH backend while the session is connected. Binds default
//! to loopback (DEC-0011).

use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};

/// One forward of a saved session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PortForward {
    /// Listen locally on `bind:bind_port`; relay each connection to
    /// `target_host:target_port` as seen from the remote side.
    Local {
        #[serde(default = "loopback")]
        bind: IpAddr,
        bind_port: u16,
        target_host: String,
        target_port: u16,
    },
    /// Ask the server to listen on `bind_host:bind_port` (0 = server picks);
    /// relay each incoming connection to the local `target_host:target_port`.
    Remote {
        #[serde(default = "loopback_host")]
        bind_host: String,
        bind_port: u16,
        target_host: String,
        target_port: u16,
    },
    /// A local SOCKS5 proxy (no authentication, CONNECT only) on `bind:bind_port`
    /// whose destinations are resolved on the remote side.
    Dynamic {
        #[serde(default = "loopback")]
        bind: IpAddr,
        bind_port: u16,
    },
}

/// The default bind address: loopback (DEC-0011).
pub fn loopback() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn loopback_host() -> String {
    "127.0.0.1".to_string()
}

/// Why a forward specification is not usable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortForwardError {
    #[error("the listening port must be 1..65535")]
    BindPort,
    #[error("the target port must be 1..65535")]
    TargetPort,
    #[error("the target host must not be empty")]
    EmptyHost,
    #[error("hosts must not contain whitespace")]
    Whitespace,
}

impl PortForward {
    /// `Local` / `Remote` / `Dynamic`, the dialog's kind label.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Local { .. } => "Local",
            Self::Remote { .. } => "Remote",
            Self::Dynamic { .. } => "Dynamic",
        }
    }

    /// Reject ports out of range and empty or whitespace hosts. A `Remote`
    /// listening port of 0 is allowed: the server picks one.
    pub fn validate(&self) -> Result<(), PortForwardError> {
        let host_ok = |host: &str| {
            if host.is_empty() {
                Err(PortForwardError::EmptyHost)
            } else if host.chars().any(char::is_whitespace) {
                Err(PortForwardError::Whitespace)
            } else {
                Ok(())
            }
        };
        match self {
            Self::Local {
                bind_port,
                target_host,
                target_port,
                ..
            } => {
                if *bind_port == 0 {
                    return Err(PortForwardError::BindPort);
                }
                if *target_port == 0 {
                    return Err(PortForwardError::TargetPort);
                }
                host_ok(target_host)
            }
            Self::Remote {
                bind_host,
                target_host,
                target_port,
                ..
            } => {
                if *target_port == 0 {
                    return Err(PortForwardError::TargetPort);
                }
                host_ok(bind_host)?;
                host_ok(target_host)
            }
            Self::Dynamic { bind_port, .. } => {
                if *bind_port == 0 {
                    Err(PortForwardError::BindPort)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// The listening side, `(kind, address, port)`, used to reject duplicates.
    pub fn bind_key(&self) -> (&'static str, String, u16) {
        match self {
            Self::Local {
                bind, bind_port, ..
            }
            | Self::Dynamic { bind, bind_port } => {
                (self.kind_label(), bind.to_string(), *bind_port)
            }
            Self::Remote {
                bind_host,
                bind_port,
                ..
            } => (self.kind_label(), bind_host.clone(), *bind_port),
        }
    }

    /// Whether the listening side is confined to the local machine.
    pub fn binds_loopback(&self) -> bool {
        match self {
            Self::Local { bind, .. } | Self::Dynamic { bind, .. } => bind.is_loopback(),
            Self::Remote { bind_host, .. } => {
                bind_host == "127.0.0.1" || bind_host == "::1" || bind_host == "localhost"
            }
        }
    }

    /// One-line description, e.g. `L 127.0.0.1:8080 -> localhost:80`.
    pub fn summary(&self) -> String {
        match self {
            Self::Local {
                bind,
                bind_port,
                target_host,
                target_port,
            } => format!("L {bind}:{bind_port} -> {target_host}:{target_port}"),
            Self::Remote {
                bind_host,
                bind_port,
                target_host,
                target_port,
            } => format!("R {bind_host}:{bind_port} -> {target_host}:{target_port}"),
            Self::Dynamic { bind, bind_port } => format!("D {bind}:{bind_port} (SOCKS5)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(bind_port: u16, target_host: &str, target_port: u16) -> PortForward {
        PortForward::Local {
            bind: loopback(),
            bind_port,
            target_host: target_host.into(),
            target_port,
        }
    }

    #[test]
    fn validation_rejects_bad_ports_and_hosts() {
        assert_eq!(
            local(0, "h", 80).validate(),
            Err(PortForwardError::BindPort)
        );
        assert_eq!(
            local(8080, "h", 0).validate(),
            Err(PortForwardError::TargetPort)
        );
        assert_eq!(
            local(8080, "", 80).validate(),
            Err(PortForwardError::EmptyHost)
        );
        assert_eq!(
            local(8080, "a b", 80).validate(),
            Err(PortForwardError::Whitespace)
        );
        assert_eq!(local(8080, "localhost", 80).validate(), Ok(()));

        let remote = PortForward::Remote {
            bind_host: "127.0.0.1".into(),
            bind_port: 0,
            target_host: "127.0.0.1".into(),
            target_port: 3000,
        };
        assert_eq!(remote.validate(), Ok(()), "the server may pick the port");
        let dynamic = PortForward::Dynamic {
            bind: loopback(),
            bind_port: 1080,
        };
        assert_eq!(dynamic.validate(), Ok(()));
    }

    #[test]
    fn serde_round_trips_every_kind_and_defaults_the_bind() {
        let forwards = vec![
            local(8080, "localhost", 80),
            PortForward::Remote {
                bind_host: "0.0.0.0".into(),
                bind_port: 9000,
                target_host: "127.0.0.1".into(),
                target_port: 3000,
            },
            PortForward::Dynamic {
                bind: loopback(),
                bind_port: 1080,
            },
        ];
        let json = serde_json::to_string(&forwards).unwrap();
        assert!(json.contains("\"kind\":\"local\""), "{json}");
        let back: Vec<PortForward> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, forwards);

        let defaulted: PortForward =
            serde_json::from_str(r#"{"kind":"dynamic","bind_port":1080}"#).unwrap();
        assert_eq!(
            defaulted,
            PortForward::Dynamic {
                bind: loopback(),
                bind_port: 1080
            }
        );
        assert!(defaulted.binds_loopback());
        assert!(!back[1].binds_loopback());
    }

    #[test]
    fn summary_and_bind_key_name_the_listening_side() {
        assert_eq!(
            local(8080, "localhost", 80).summary(),
            "L 127.0.0.1:8080 -> localhost:80"
        );
        assert_eq!(
            local(8080, "localhost", 80).bind_key(),
            ("Local", "127.0.0.1".to_string(), 8080)
        );
        assert_eq!(
            PortForward::Dynamic {
                bind: loopback(),
                bind_port: 1080
            }
            .summary(),
            "D 127.0.0.1:1080 (SOCKS5)"
        );
    }
}
