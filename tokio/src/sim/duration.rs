use super::SimTime;
use std::ops::*;

pub use std::time::Duration;

// THIRD PARTY TYPES

impl Add<Duration> for SimTime {
    type Output = SimTime;

    fn add(self, rhs: Duration) -> Self::Output {
        self.checked_add(rhs)
            .expect("Overflow when adding Duration to SimTime")
    }
}

impl AddAssign<Duration> for SimTime {
    fn add_assign(&mut self, rhs: Duration) {
        self.0.add_assign(rhs)
    }
}
