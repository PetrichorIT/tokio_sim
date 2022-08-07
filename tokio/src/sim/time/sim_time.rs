use std::cell::Cell;
use std::f64::EPSILON;
use std::fmt::*;
use std::ops::*;

use super::Duration;

thread_local!(pub(crate) static SIMTIME: Cell<SimTime> = const { Cell::new(SimTime::ZERO) });

///
/// A specific point of time in the simulation.
///
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimTime(pub(super) Duration);

impl SimTime {
    ///
    /// Creates a raw simtime from a duration
    ///
    pub const fn from_duration(dur: Duration) -> Self {
        Self(dur)
    }

    /// Returns an instant corresponding to "now" in the simulation context.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use osaka::time::SimTime;
    ///
    /// let now = SimTime::now();
    /// ```
    #[must_use]
    pub fn now() -> Self {
        SIMTIME.with(|s| s.get())
    }

    ///
    /// Sets the current clock
    ///
    pub fn set_now(time: SimTime) {
        SIMTIME.with(|s| s.set(time));
    }

    ///
    /// Performs a equallity check with a error margin.
    ///
    pub fn eq_approx(&self, other: SimTime, error: Duration) -> bool {
        let dur = self.duration_diff(other);
        dur < error
    }

    /// Retursn the amount of time elapsed from the earlier of the two values
    /// to the higher.
    #[must_use]
    pub fn duration_diff(&self, other: SimTime) -> Duration {
        if *self > other {
            self.duration_since(other)
        } else {
            other.duration_since(*self)
        }
    }

    /// Returns the amount of time elapsed from another instant to this one,
    /// or zero duration if that instant is later than this one.
    #[must_use]
    pub fn duration_since(&self, earlier: SimTime) -> Duration {
        self.checked_duration_since(earlier).unwrap()
    }

    /// Returns the amount of time elapsed from another instant to this one,
    /// or None if that instant is later than this one.
    #[must_use]
    pub fn checked_duration_since(&self, earlier: SimTime) -> Option<Duration> {
        self.0.checked_sub(earlier.0)
    }

    /// Returns the amount of time elapsed from another instant to this one,
    /// or zero duration if that instant is later than this one.
    #[must_use]
    pub fn saturating_duration_since(&self, earlier: SimTime) -> Duration {
        self.checked_duration_since(earlier).unwrap_or_default()
    }

    /// Returns the amount of time elapsed since this instant was created.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        Self::now() - *self
    }

    /// Returns `Some(t)` where `t` is the time `self + duration` if `t` can be represented as
    /// `Instant` (which means it's inside the bounds of the underlying data structure), `None`
    /// otherwise.
    #[must_use]
    pub fn checked_add(&self, duration: Duration) -> Option<SimTime> {
        self.0.checked_add(duration).map(SimTime)
    }

    /// Returns `Some(t)` where `t` is the time `self - duration` if `t` can be represented as
    /// `Instant` (which means it's inside the bounds of the underlying data structure), `None`
    /// otherwise.
    #[must_use]
    pub fn checked_sub(&self, duration: Duration) -> Option<SimTime> {
        self.0.checked_sub(duration).map(SimTime)
    }
}

// # Custom Additions
impl SimTime {
    /// The smallest instance of a [SimTime].
    pub const ZERO: SimTime = SimTime(Duration::ZERO);
    /// One second instance of a [SimTime].
    pub const ONE: SimTime = SimTime(Duration::new(1, 0));
    /// The smallest valid instance of a [SimTime].
    pub const MIN: SimTime = SimTime(Duration::ZERO);
    /// The greatest instance of a [SimTime].
    pub const MAX: SimTime = SimTime(Duration::MAX);
}

// This provides non-mutable functionality like as_secs()
impl Deref for SimTime {
    type Target = Duration;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// CMP

impl PartialEq<f64> for SimTime {
    fn eq(&self, other: &f64) -> bool {
        let diff = (self.0.as_secs_f64() - *other).abs();
        diff < EPSILON
    }
}

// OPS

impl Sub<Duration> for SimTime {
    type Output = SimTime;

    fn sub(self, rhs: Duration) -> Self::Output {
        self.checked_sub(rhs)
            .expect("Overflow when substracting Duration from SimTime")
    }
}

impl SubAssign<Duration> for SimTime {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs
    }
}

impl Sub<SimTime> for SimTime {
    type Output = Duration;

    fn sub(self, rhs: SimTime) -> Self::Output {
        self.duration_since(rhs)
    }
}

impl Div<SimTime> for SimTime {
    type Output = f64;

    fn div(self, rhs: SimTime) -> Self::Output {
        self.0.as_secs_f64() / rhs.0.as_secs_f64()
    }
}

impl Div<f64> for SimTime {
    type Output = SimTime;

    fn div(self, rhs: f64) -> Self::Output {
        Self::from(self.0.as_secs_f64() / rhs)
    }
}

// FMT

impl Debug for SimTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Display for SimTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

// FROM

impl From<SimTime> for f64 {
    fn from(this: SimTime) -> Self {
        this.0.as_secs_f64()
    }
}

impl From<f64> for SimTime {
    fn from(value: f64) -> Self {
        SimTime(Duration::from_secs_f64(value))
    }
}

impl Add<SimTime> for SimTime {
    type Output = SimTime;
    fn add(self, rhs: SimTime) -> SimTime {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign<SimTime> for SimTime {
    fn add_assign(&mut self, rhs: SimTime) {
        self.0.add_assign(rhs.0)
    }
}

// f64

impl Add<f64> for SimTime {
    type Output = SimTime;

    fn add(self, rhs: f64) -> Self::Output {
        self.checked_add(Duration::from_secs_f64(rhs))
            .expect("Overflow when adding Duration to SimTime")
    }
}

impl AddAssign<f64> for SimTime {
    fn add_assign(&mut self, rhs: f64) {
        self.0.add_assign(Duration::from_secs_f64(rhs))
    }
}

impl Default for SimTime {
    fn default() -> Self {
        Self::from(0.0)
    }
}
