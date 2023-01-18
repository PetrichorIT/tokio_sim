use std::sync::{Condvar, Mutex};

pub(crate) struct Monitor {
    mutex: Mutex<usize>,
    cvar: Condvar,
    // size: usize,
}

impl Monitor {
    pub(crate) fn new(_size: usize) -> Self {
        println!("creating {} worker threads", _size);
        Self {
            mutex: Mutex::new(0),
            cvar: Condvar::new(),
            // size,
        }
    }

    pub(crate) fn activate_worker(&self) {
        *self.mutex.lock().unwrap() += 1;
    }

    pub(crate) fn deactivate_worker(&self) {
        *self.mutex.lock().unwrap() -= 1;
        self.cvar.notify_one();
    }

    pub(crate) fn await_idle(&self) {
        let mut active = self.mutex.lock().unwrap();
        while *active > 0 {
            active = self.cvar.wait(active).unwrap();
        }
    }
}
