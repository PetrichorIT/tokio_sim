use std::cell::RefCell;

thread_local! {
    pub(crate) static IOCTX: RefCell<SimContext> = const { RefCell::new(SimContext::empty()) }
}

/// The IO Contexxt
#[derive(Debug)]
pub struct SimContext {
    /// The IO Context
    #[cfg(feature = "net")]
    pub io: Option<IOContext>,

    /// The IO Context
    pub time: Option<TimeContext>,
}

impl SimContext {
    /// The intial sim_contex without subsequent entries.
    pub const fn empty() -> Self {
        Self {
            #[cfg(feature = "net")]
            io: None,

            time: None,
        }
    }

    /// fetch the current context
    pub fn with_current<R>(f: impl FnOnce(&mut SimContext) -> R) -> R {
        IOCTX.with(|v| f(&mut *v.borrow_mut()))
    }

    /// With a dummy IO Context.
    pub fn with_io(mut self) -> Self {
        self.io = Some(IOContext::empty());
        self
    }

    /// With time
    pub fn with_time(mut self, ident: String) -> Self {
        self.time = Some(TimeContext::new(ident));
        self
    }

    /// Creates a new context
    #[cfg(feature = "net")]
    pub fn new(ether: [u8; 6], v4: Ipv4Addr) -> Self {
        Self {
            io: Some(IOContext::new(ether, v4)),
            time: None,
        }
    }

    /// Resets the SimContext after module restart.
    pub fn reset(&mut self) {
        self.io.as_mut().map(|io| io.reset());
        self.time.as_mut().map(|time| time.reset());
    }

    /// Swaps out the current context
    pub(crate) fn swap(other: &mut SimContext) {
        IOCTX.with(|c| {
            std::mem::swap(other, &mut *c.borrow_mut());
        });
    }
}

cfg_net! {
    use super::net::IOContext;
    use std::net::Ipv4Addr;
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

        #[cfg(feature = "net")]
        {
            writeln!(f, "[[net]]")?;
            writeln!(
                f,
                "sockets = {:?}",
                self.io.as_ref().map(|v| &v.udp_sockets)
            )?;
        }

        Ok(())
    }
}
