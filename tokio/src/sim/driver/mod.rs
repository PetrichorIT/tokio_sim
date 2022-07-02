use super::SimTime;
use std::cell::Cell;
use std::cell::{RefCell, RefMut};
use std::collections::BinaryHeap;
use std::ops::DerefMut;
use std::task::{Context, Waker};

mod sleep;
pub use sleep::*;

mod interval;
pub use interval::*;

mod timeout;
pub use timeout::*;

thread_local! {
    pub(crate) static TIME_DRIVER: RefCell<Option<Box<TimeDriver>>> = RefCell::new(None);
}

#[derive(Debug, Default)]
pub(crate) struct TimeDriver {
    sleeps: BinaryHeap<Entry>,
    next_wakeup_emitted: Cell<Option<SimTime>>,
}

unsafe impl Send for TimeDriver {}
unsafe impl Sync for TimeDriver {}

impl TimeDriver {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn swap_context(new_driver: Box<TimeDriver>) -> Option<Box<TimeDriver>> {
        TIME_DRIVER.with(|driver| driver.borrow_mut().replace(new_driver))
    }

    pub(crate) fn is_context_set() -> bool {
        TIME_DRIVER.with(|b| b.borrow().is_some())
    }

    pub(crate) fn unset_context() -> Box<TimeDriver> {
        TIME_DRIVER
            .with(|driver| driver.borrow_mut().take())
            .expect("Failed to fetch driver from current context")
    }

    pub(crate) fn wake_sleeper(&mut self, source: &Sleep, cx: &mut Context<'_>) {
        assert!(source.deadline >= SimTime::now());

        if self.sleeps.iter().any(|e| e.id == source.id) {
            return;
        }

        self.sleeps.push(Entry {
            deadline: source.deadline,
            id: source.id,
            waker: cx.waker().clone(),
        })
    }

    pub(crate) fn reset_sleeper(&mut self, id: usize, new_deadline: SimTime) {
        let mut heap = BinaryHeap::new();
        std::mem::swap(&mut heap, &mut self.sleeps);

        let mut drained_heap = heap.into_vec();
        if let Some(entry) = drained_heap.iter_mut().find(|v| v.id == id) {
            entry.deadline = new_deadline
        } else {
            // panic!("Unknown sleeper")
        }

        let mut new_heap = BinaryHeap::from(drained_heap);
        std::mem::swap(&mut new_heap, &mut self.sleeps);
        // drop(new_heap) == drop(heap) == drop(BinaryHeap::new())
    }

    pub(crate) fn with_current<R>(f: impl FnOnce(RefMut<'_, Self>) -> R) -> R {
        TIME_DRIVER.with(|c| {
            let driver = RefMut::map(c.borrow_mut(), |v| {
                v.as_mut()
                    .expect("Failed to take driver from current context")
                    .deref_mut() // to resolve the boxing
            });

            // Execute function with time driver.
            f(driver)
        })
    }

    pub(crate) fn next_wakeup(&self) -> Option<SimTime> {
        let c = self.sleeps.peek().map(|v| v.deadline);
        match (c, self.next_wakeup_emitted.get()) {
            // Nothing to emit
            (None, _) => None,
            // If we want to emit the same data again dont do it
            // TODO: maybe some checks with SimTime::now() ?
            (Some(c), Some(p)) if p == c => None,
            // This case catches prev == None in which case we are at inital set
            // or prev == Some(p) with p < or greater that c
            // greates is not possible so smaller so overwite the emit
            // an reemit the event
            _ => {
                // Inital set or overwrite ste
                self.next_wakeup_emitted.set(c);
                c
            }
        }
    }

    pub(crate) fn take_timestep(&mut self) -> Vec<Waker> {
        let mut wakers = Vec::new();

        while let Some(peeked) = self.sleeps.peek() {
            if peeked.deadline == SimTime::now() {
                wakers.push(self.sleeps.pop().unwrap().waker)
            } else {
                break;
            }
        }

        wakers
    }
}

#[derive(Debug)]
struct Entry {
    deadline: SimTime,
    waker: Waker,
    id: usize,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline && self.id == other.id
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.id == other.id {
            true => std::cmp::Ordering::Equal,
            _ => other.deadline.cmp(&self.deadline),
        }
    }
}
