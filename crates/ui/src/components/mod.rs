//! UI component tái sử dụng (widget thuần, không phụ thuộc domain).

pub mod datetime_clock;
pub mod net_speed;

pub use datetime_clock::DateTimeClock;
pub use net_speed::NetSpeedIndicator;