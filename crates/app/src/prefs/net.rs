//! The network's settings (specs/oneterm, "Network"): the proxy and whether certificates are
//! checked, for what the app fetches over HTTP, which is its updates.

use quadra::{Settings, Signal};
use quadra_updater::Network;

/// Where the proxy is from: the one Windows is set to use, or one named here.
pub const PROXIES: [&str; 2] = ["System", "Manual"];

#[derive(Clone, Copy)]
pub struct NetPrefs {
    /// The index in [`PROXIES`].
    pub proxy: Signal<usize>,
    /// The proxy named by hand, `host:port`.
    pub proxy_address: Signal<String>,
    pub verify_tls: Signal<bool>,
}

impl NetPrefs {
    pub(crate) fn of(s: &Settings) -> NetPrefs {
        NetPrefs {
            proxy: s.choice("network.proxy", &PROXIES),
            proxy_address: s.value("network.proxy_address", String::new()),
            verify_tls: s.value("network.verify_tls", true),
        }
    }

    /// As a fetch takes them, now. Manual with no address is the system's.
    pub fn network(&self) -> Network {
        let named = self.proxy_address.get_untracked();
        let manual = self.proxy.get_untracked() == 1 && !named.trim().is_empty();
        Network { proxy: manual.then_some(named), verify_tls: self.verify_tls.get_untracked() }
    }
}
