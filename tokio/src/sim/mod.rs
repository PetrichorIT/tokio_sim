//! A collection of changes used when feature "sim" is active.

cfg_time! {
    /// Utilities for tracking time with feature `sim`.
    pub mod time;
}

mod ctx;
pub use ctx::*;
