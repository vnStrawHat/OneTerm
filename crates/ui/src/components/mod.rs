//! Reusable UI components (pure widgets, no domain dependencies).

pub mod datetime_clock;
pub mod net_speed;
pub mod resource;

pub use datetime_clock::DateTimeClock;
pub use net_speed::NetSpeedIndicator;
pub use resource::ResourceIndicator;
