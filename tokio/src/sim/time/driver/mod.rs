use super::clock::Clock;
use super::Duration;
use super::SimTime;
use crate::loom::sync::{atomic::*, Arc, Mutex};
use crate::park::{Park, Unpark};

pub(crate) mod handle;
pub(crate) use handle::*;

mod sleep;
pub use sleep::*;

mod interval;
pub use interval::*;

mod timeout;
pub use timeout::*;

mod queue;
use queue::*;

#[derive(Debug)]
pub(crate) struct Driver<P: Park + 'static> {
    /// Timing backend in use.
    #[allow(unused)]
    time_source: ClockTime,

    /// Shared state.
    handle: Handle,

    /// Delegate
    #[allow(unused)]
    park: P,
}

impl<P: Park + 'static> Driver<P> {
    pub(crate) fn new(park: P, clock: Clock) -> Driver<P> {
        let time_source = ClockTime::new(clock);
        let inner = Inner::new(time_source.clone(), Box::new(park.unpark()));

        Driver {
            time_source,
            handle: Handle::new(Arc::new(inner)),
            park,
        }
    }

    pub(crate) fn handle(&self) -> Handle {
        self.handle.clone()
    }

    pub(self) fn park_internal(&self, _v: Option<Duration>) -> Result<(), <Self as Park>::Error> {
        // NOP
        Ok(())
    }
}

impl Handle {
    pub(crate) fn next_time_poll(&self) -> Option<SimTime> {
        self.get().lock().ctx.queue.next_wakeup()
    }

    pub(crate) fn process_now(&self) {
        let now = SimTime::now();
        self.process_at(now)
    }

    pub(crate) fn process_at(&self, now: SimTime) {
        // fetch the slot for the current timepoint.
        let lock = self.get().lock();

        for time_slot in lock.ctx.queue.pop(now) {
            time_slot.wake_all();
        }
    }
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub(self) struct ClockTime {
    clock: super::clock::Clock,
    start_time: SimTime,
}

impl ClockTime {
    pub(self) fn new(clock: Clock) -> Self {
        Self {
            start_time: clock.now(),
            clock,
        }
    }
}

/// Timer state shared between `Driver`, `Handle`, and `Registration`.
pub(crate) struct Inner {
    // The state is split like this so `Handle` can access `is_shutdown` without locking the mutex
    pub(super) state: Mutex<InnerState>,

    pub(super) is_shutdown: AtomicBool,
}

impl Inner {
    pub(self) fn new(time_source: ClockTime, unpark: Box<dyn Unpark>) -> Inner {
        Inner {
            state: Mutex::new(InnerState {
                time_source,
                ctx: TimeContext {
                    ident: String::from("AsInner"),
                    queue: Arc::new(TimerQueue::new(SimTime::now())),
                },
                unpark,
            }),
            is_shutdown: AtomicBool::new(false),
        }
    }

    /// Locks the driver's inner structure
    pub(crate) fn lock(&self) -> crate::loom::sync::MutexGuard<'_, InnerState> {
        self.state.lock()
    }

    // Check whether the driver has been shutdown
    pub(super) fn is_shutdown(&self) -> bool {
        self.is_shutdown.load(Ordering::SeqCst)
    }
}

/// Time state shared which must be protected by a `Mutex`
pub(crate) struct InnerState {
    /// Timing backend in use.
    #[allow(unused)]
    time_source: ClockTime,

    // Timer queue to handle wakeups
    pub(crate) ctx: TimeContext,

    #[allow(unused)]
    unpark: Box<dyn Unpark>,
}

impl InnerState {
    pub(crate) fn swap_ctx(&mut self, other: &mut TimeContext) {
        std::mem::swap(&mut self.ctx, other)
    }
}

impl<P> Park for Driver<P>
where
    P: Park + 'static,
{
    type Unpark = TimerUnpark<P>;
    type Error = P::Error;

    fn unpark(&self) -> Self::Unpark {
        TimerUnpark::new(self)
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.park_internal(None)
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.park_internal(Some(duration))
    }

    fn shutdown(&mut self) {
        if self.handle.is_shutdown() {
            return;
        }

        self.handle.get().is_shutdown.store(true, Ordering::SeqCst);

        // Advance time forward to the end of time.

        // Rethink is this a good idea
        // self.handle.process_at(SimTime::MAX);

        self.park.shutdown();
    }
}

impl<P> Drop for Driver<P>
where
    P: Park + 'static,
{
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) struct TimerUnpark<P: Park + 'static> {
    inner: P::Unpark,
}

impl<P: Park + 'static> TimerUnpark<P> {
    fn new(driver: &Driver<P>) -> TimerUnpark<P> {
        TimerUnpark {
            inner: driver.park.unpark(),
        }
    }
}

impl<P: Park + 'static> Unpark for TimerUnpark<P> {
    fn unpark(&self) {
        self.inner.unpark();
    }
}

///
/// The context holding all time sensitiv futures of a runtime.
///
#[derive(Debug)]
pub struct TimeContext {
    ident: String,
    queue: Arc<TimerQueue>,
}

impl TimeContext {
    /// Creates a new time context.
    pub fn new(ident: String) -> Self {
        Self {
            ident,
            queue: Arc::new(TimerQueue::new(SimTime::now())),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.queue.reset();
    }

    /// An identifier for the time context. Empty string if default time context is used.
    pub fn ident(&self) -> String {
        self.ident.clone()
    }

    /// next_time_poll
    pub fn next_time_poll(&self) -> Option<SimTime> {
        self.queue.next_wakeup()
    }

    /// process_now
    pub fn process_now(&self) {
        let now = SimTime::now();
        self.process_at(now)
    }

    /// process_at
    pub fn process_at(&self, now: SimTime) {
        // fetch the slot for the current timepoint.
        for time_slot in self.queue.pop(now) {
            time_slot.wake_all();
        }
    }

    /// swap
    pub fn swap(&mut self, other: &mut TimeContext) {
        std::mem::swap(&mut self.ident, &mut other.ident);
        self.queue.swap(&other.queue)
    }
}
