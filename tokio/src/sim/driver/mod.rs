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
        self.get().lock().queue.next_wakeup()
    }

    pub(crate) fn process_now(&self) {
        let now = SimTime::now();
        self.process_at(now)
    }

    pub(crate) fn process_at(&self, now: SimTime) {
        // fetch the slot for the current timepoint.
        let lock = self.get().lock();
   
        for time_slot in lock.queue.pop(now) {
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
struct Inner {
    // The state is split like this so `Handle` can access `is_shutdown` without locking the mutex
    pub(super) state: Mutex<InnerState>,

    pub(super) is_shutdown: AtomicBool,
}

impl Inner {
    pub(super) fn new(time_source: ClockTime, unpark: Box<dyn Unpark>) -> Inner {
        Inner {
            state: Mutex::new(InnerState {
                time_source,
                queue: Arc::new(TimerQueue::new(SimTime::now())),
                unpark,
            }),
            is_shutdown: AtomicBool::new(false),
        }
    }

    /// Locks the driver's inner structure
    pub(super) fn lock(&self) -> crate::loom::sync::MutexGuard<'_, InnerState> {
        self.state.lock()
    }

    // Check whether the driver has been shutdown
    pub(super) fn is_shutdown(&self) -> bool {
        self.is_shutdown.load(Ordering::SeqCst)
    }
}

/// Time state shared which must be protected by a `Mutex`
struct InnerState {
    /// Timing backend in use.
    #[allow(unused)]
    time_source: ClockTime,

    // Timer queue to handle wakeups
    queue: Arc<TimerQueue>,

    #[allow(unused)]
    unpark: Box<dyn Unpark>,
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

        self.handle.process_at(SimTime::MAX);

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

