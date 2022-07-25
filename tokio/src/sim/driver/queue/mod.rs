use std::cell::RefCell;
use std::cell::Cell;
use std::cmp::{Eq, PartialEq};
use std::collections::VecDeque;
use std::task::Waker;

use crate::loom::sync::{Arc, Weak};
use crate::sim::SimTime;

#[derive(Debug)]
pub(super) struct TimerQueue {
    current: Cell<SimTime>,
    pending: RefCell<VecDeque<Arc<TimeSlot>>>,
}

impl TimerQueue {
    pub(crate) fn new(time: SimTime) -> Self {
        Self {
            current: Cell::new(time),
            pending: RefCell::new(VecDeque::new()),
        }
    }

    pub(crate) fn push(
        self: &Arc<TimerQueue>,
        entry: TimeSlotEntry,
        time: SimTime,
    ) -> TimeSlotEntryHandle {
        assert!(time >= self.current.get());
        let mut pending = self.pending.borrow_mut();
        let id = entry.id;

        // Binary search for slot
        match pending.binary_search_by(|slot| slot.slot.cmp(&time))
        {
            Ok(found) => {
                pending[found].push(entry);
                TimeSlotEntryHandle {
                    id,
                    handle: Arc::downgrade(&pending[found]),
                }
            }
            Err(insert_at) => {
                pending.insert(
                    insert_at,
                    Arc::new(TimeSlot {
                        slot: time,
                        entries: RefCell::new(vec![entry]),

                        queue: self.clone(),
                    }),
                );
                TimeSlotEntryHandle {
                    id,
                    handle: Arc::downgrade(&pending[insert_at]),
                }
            }
        }
    }

    pub(crate) fn next_wakeup(&self) -> Option<SimTime> {
        Some(self.pending.borrow().front()?.slot)
    }

    pub(crate) fn pop(&self, now: SimTime) -> Vec<TimeSlot> {
        assert!(now >= self.current.get());
        self.current.set(now);
        let front = match self.pending.borrow().front() {
            Some(v) => v.slot,
            None => return Vec::new()
        };
        if front <= now {
            let mut buffer = Vec::new();
            while let Some(Ok(v)) = self.pending.borrow_mut().pop_front().map(Arc::try_unwrap) {
                buffer.push(v)
            }
            buffer
        } else {
            Vec::new()
        }
    }
}

// SAFTEY
// All components are Send and Sync except RefCell, but since sim
// implies a single current-thread runtime this is also save.
// Additionally this typ is only used through an Arc
unsafe impl Send for TimerQueue {}
unsafe impl Sync for TimerQueue {}

/// All events scheduled for a certain time slot
///
/// Note that in simulation context the slot is just
/// one point in time.
#[derive(Debug)]
pub(super) struct TimeSlot {
    pub(super) slot: SimTime,
    pub(super) entries: RefCell<Vec<TimeSlotEntry>>,

    pub(super) queue: Arc<TimerQueue>,
}

impl TimeSlot {
    pub(crate) fn push(&self, entry: TimeSlotEntry) {
        let mut entries = self.entries.borrow_mut();
        if !entries.iter().any(|v| v.id == entry.id) {
            entries.push(entry)
        }
    }

    pub(crate) fn remove(&self, id: usize) -> Option<TimeSlotEntry> {
        let mut entries = self.entries.borrow_mut();
        for i in 0..entries.len() {
            if entries[i].id == id {
                return Some(entries.remove(i));
            }
        }

        None
    }

    pub(crate) fn wake_all(self) {
        self.entries.into_inner().into_iter().for_each(|entry| entry.waker.wake())
    }
}

// SAFTEY
// A Time slot can be trivialy Send since it does not point to shared
// data without using Send save primives (Arc), and it can be Sync
// since usage of sim implies single threaded usage and no long
// term references are taken of this internal datatyp.
unsafe impl Send for TimeSlot {}
unsafe impl Sync for TimeSlot {}

impl PartialEq for TimeSlot {
    fn eq(&self, other: &Self) -> bool {
        self.slot == other.slot
    }
}

impl Eq for TimeSlot {}

impl PartialOrd for TimeSlot {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimeSlot {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self == other {
            true => std::cmp::Ordering::Equal,
            _ => other.slot.cmp(&self.slot),
        }
    }
}

/// A waker associated to a sleeper.
#[derive(Debug)]
pub(super) struct TimeSlotEntry {
    pub(super) waker: Waker,
    pub(super) id: usize,
}

impl PartialEq for TimeSlotEntry {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TimeSlotEntry {}

#[derive(Debug)]
pub(super) struct TimeSlotEntryHandle {
    pub(super) id: usize,
    pub(super) handle: Weak<TimeSlot>,
}

impl TimeSlotEntryHandle {
    pub(crate) fn reset(self, new_deadline: SimTime) -> Option<TimeSlotEntryHandle> {
        let handle = self.handle.upgrade()?;
        let entry = handle.remove(self.id)?;

        Some(handle.queue.push(entry, new_deadline))
    }
}
