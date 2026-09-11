//! Reusable UI components (pure widgets, no domain dependencies).

pub mod breadcrumb;
pub mod datetime_clock;
pub mod git_status;
pub mod net_speed;
pub mod resource;
pub mod status_text;

pub use breadcrumb::breadcrumb;
pub use datetime_clock::datetime_clock;
pub use git_status::git_status;
pub use net_speed::net_speed;
pub use resource::resource;
pub use status_text::StatusText;
