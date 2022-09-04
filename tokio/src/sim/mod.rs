//! A collection of changes used when feature "sim" is active.

cfg_time! {
    /// Utilities for tracking time with feature `sim`.
    pub mod time;
}

cfg_net! {
    /// TCP/UDP/Unix bindings for `tokio` with feature `sim`.
    pub mod net;
}

mod ctx;
pub use ctx::*;
