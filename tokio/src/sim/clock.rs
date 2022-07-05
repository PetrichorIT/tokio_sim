//! Source of time abstraction.
//!
//! By default, `std::time::Instant::now()` is used. However, when the
//! `test-util` feature flag is enabled, the values returned for `now()` are
//! configurable.
//!
use crate::time::SimTime;

#[derive(Debug, Clone)]
pub(crate) struct Clock {}

pub(crate) fn now() -> SimTime {
    SimTime::now()
}

impl Clock {
    pub(crate) fn new(_enable_pausing: bool, _start_paused: bool) -> Clock {
        Clock {}
    }

    pub(crate) fn now(&self) -> SimTime {
        now()
    }
}
