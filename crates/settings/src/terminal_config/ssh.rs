//! SSH connection-liveness settings.

use serde::{Deserialize, Serialize};

use oneterm_core::{
    DEFAULT_SSH_KEEPALIVE_INTERVAL_SECS, DEFAULT_SSH_KEEPALIVE_MAX, SshKeepaliveConfig,
};

/// Global SSH settings applied when a new connection starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshSettingsConfig {
    /// Send SSH transport keepalive requests while the connection is idle.
    #[serde(default = "default_keepalive_enabled")]
    pub keepalive_enabled: bool,
    /// Seconds between keepalive requests.
    #[serde(default = "default_keepalive_interval_secs")]
    pub keepalive_interval_secs: u64,
    /// Maximum unanswered keepalive requests tolerated before disconnecting.
    #[serde(default = "default_keepalive_max")]
    pub keepalive_max: usize,
}

impl SshSettingsConfig {
    /// Build the validated runtime policy used by the SSH backend.
    pub fn keepalive(&self) -> SshKeepaliveConfig {
        SshKeepaliveConfig::new(
            self.keepalive_enabled,
            self.keepalive_interval_secs,
            self.keepalive_max,
        )
    }
}

impl Default for SshSettingsConfig {
    fn default() -> Self {
        Self {
            keepalive_enabled: default_keepalive_enabled(),
            keepalive_interval_secs: default_keepalive_interval_secs(),
            keepalive_max: default_keepalive_max(),
        }
    }
}

const fn default_keepalive_enabled() -> bool {
    true
}

const fn default_keepalive_interval_secs() -> u64 {
    DEFAULT_SSH_KEEPALIVE_INTERVAL_SECS
}

const fn default_keepalive_max() -> usize {
    DEFAULT_SSH_KEEPALIVE_MAX
}

#[cfg(test)]
mod tests {
    use oneterm_core::{
        MAX_SSH_KEEPALIVE_INTERVAL_SECS, MAX_SSH_KEEPALIVE_MAX, MIN_SSH_KEEPALIVE_INTERVAL_SECS,
        MIN_SSH_KEEPALIVE_MAX,
    };

    use super::*;

    #[test]
    fn missing_fields_keep_the_safe_existing_defaults() {
        let config: SshSettingsConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, SshSettingsConfig::default());
        assert!(config.keepalive().enabled());
        assert_eq!(
            config.keepalive().interval_secs(),
            DEFAULT_SSH_KEEPALIVE_INTERVAL_SECS
        );
        assert_eq!(config.keepalive().max(), DEFAULT_SSH_KEEPALIVE_MAX);
    }

    #[test]
    fn runtime_policy_normalizes_out_of_range_values() {
        let below = SshSettingsConfig {
            keepalive_interval_secs: 0,
            keepalive_max: 0,
            ..SshSettingsConfig::default()
        };
        let above = SshSettingsConfig {
            keepalive_interval_secs: u64::MAX,
            keepalive_max: usize::MAX,
            ..SshSettingsConfig::default()
        };
        assert_eq!(
            below.keepalive().interval_secs(),
            MIN_SSH_KEEPALIVE_INTERVAL_SECS
        );
        assert_eq!(
            above.keepalive().interval_secs(),
            MAX_SSH_KEEPALIVE_INTERVAL_SECS
        );
        assert_eq!(below.keepalive().max(), MIN_SSH_KEEPALIVE_MAX);
        assert_eq!(above.keepalive().max(), MAX_SSH_KEEPALIVE_MAX);
    }
}
