use std::cell::RefCell;

thread_local! {
    pub(crate) static IOCTX: RefCell<SimContext> = const { RefCell::new(SimContext::empty()) }
}

/// The IO Contexxt
#[derive(Debug)]
pub struct SimContext {
    /// The IO Context
    pub time: Option<TimeContext>,
}

impl SimContext {
    /// The intial sim_contex without subsequent entries.
    pub const fn empty() -> Self {
        Self { time: None }
    }

    /// fetch the current context
    pub fn with_current<R>(f: impl FnOnce(&mut SimContext) -> R) -> R {
        IOCTX.with(|v| f(&mut *v.borrow_mut()))
    }

    /// With time
    pub fn with_time(mut self, ident: String) -> Self {
        self.time = Some(TimeContext::new(ident));
        self
    }

    /// Resets the SimContext after module restart.
    pub fn reset(&mut self) {
        self.time.as_mut().map(|time| time.reset());
    }

    /// Swaps out the current context
    pub(crate) fn swap(other: &mut SimContext) {
        IOCTX.with(|c| {
            std::mem::swap(other, &mut *c.borrow_mut());
        });
    }
}

cfg_time! {
    use super::time::TimeContext;
}

use std::fmt;
impl fmt::Display for SimContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[SimContext]")?;
        #[cfg(feature = "time")]
        {
            writeln!(f, "[[time]]")?;
            writeln!(
                f,
                "ident = {:?}",
                self.time.as_ref().map(|time_ctx| time_ctx.ident())
            )?;
        }

        Ok(())
    }
}
