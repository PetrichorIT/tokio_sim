//!
//! Time primitives mirroring [std::time] bound to the simulation time.
//!

pub(crate) mod clock;
pub mod error;

mod sim_time;
pub use sim_time::*;

mod duration;
pub use duration::*;

pub(crate) mod driver;

pub use driver::sleep;
pub use driver::sleep_until;
pub use driver::Sleep;

pub use driver::interval;
pub use driver::interval_at;
pub use driver::Interval;
pub use driver::MissedTickBehavior;

pub use driver::timeout;
pub use driver::timeout_at;
pub use error::Elapsed;
pub use driver::Timeout;

pub use driver::TimeContext;

/// A temporary redirect
pub type Instant = SimTime;

// Reexport
