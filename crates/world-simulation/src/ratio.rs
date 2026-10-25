use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TimeError {
    #[error("time is outside rational bounds")]
    Bounds,
    #[error("time arithmetic exceeded rational bounds")]
    Overflow,
    #[error("invalid fixed step, time scale or fixed-step budget")]
    Configuration,
    #[error("fixed step budget exceeded")]
    StepBudget,
}

/// Exact nonnegative seconds, reduced and bounded before entering the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeSpan {
    num: u64,
    den: u64,
}
impl TimeSpan {
    pub const ZERO: Self = Self { num: 0, den: 1 };
    pub const ONE: Self = Self { num: 1, den: 1 };
    pub const FIXED: Self = Self { num: 1, den: 50 };
    pub fn new(num: u64, den: u64) -> Result<Self, TimeError> {
        if den == 0 || num > 9_007_199_254_740_991 || den > 1_000_000_000_000 {
            return Err(TimeError::Bounds);
        }
        Self::reduce(u128::from(num), u128::from(den))
    }
    pub fn from_nanos(nanos: u64) -> Result<Self, TimeError> {
        Self::new(nanos, 1_000_000_000)
    }
    fn reduce(num: u128, den: u128) -> Result<Self, TimeError> {
        let (mut a, mut b) = (num, den);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let (num, den) = (num / a, den / a);
        if num > 9_007_199_254_740_991 || den > 1_000_000_000_000 {
            return Err(TimeError::Overflow);
        }
        Ok(Self {
            num: u64::try_from(num).map_err(|_| TimeError::Overflow)?,
            den: u64::try_from(den).map_err(|_| TimeError::Overflow)?,
        })
    }
    pub fn checked_add(self, other: Self) -> Result<Self, TimeError> {
        Self::reduce(
            u128::from(self.num) * u128::from(other.den)
                + u128::from(other.num) * u128::from(self.den),
            u128::from(self.den) * u128::from(other.den),
        )
    }
    pub fn checked_sub(self, other: Self) -> Result<Self, TimeError> {
        let num = (u128::from(self.num) * u128::from(other.den))
            .checked_sub(u128::from(other.num) * u128::from(self.den))
            .ok_or(TimeError::Bounds)?;
        Self::reduce(num, u128::from(self.den) * u128::from(other.den))
    }
    pub fn checked_mul(self, other: Self) -> Result<Self, TimeError> {
        Self::reduce(
            u128::from(self.num) * u128::from(other.num),
            u128::from(self.den) * u128::from(other.den),
        )
    }
    pub(crate) fn checked_div(self, other: Self) -> Result<Self, TimeError> {
        if other.num == 0 {
            return Err(TimeError::Bounds);
        }
        Self::reduce(
            u128::from(self.num) * u128::from(other.den),
            u128::from(self.den) * u128::from(other.num),
        )
    }
    pub(crate) fn steps(self, fixed: Self) -> u128 {
        (u128::from(self.num) * u128::from(fixed.den))
            / (u128::from(self.den) * u128::from(fixed.num))
    }
    #[allow(clippy::cast_precision_loss)]
    pub fn seconds(self) -> f64 {
        self.num as f64 / self.den as f64
    }
}
impl PartialOrd for TimeSpan {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TimeSpan {
    fn cmp(&self, other: &Self) -> Ordering {
        (u128::from(self.num) * u128::from(other.den))
            .cmp(&(u128::from(other.num) * u128::from(self.den)))
    }
}

impl Default for TimeSpan {
    fn default() -> Self {
        Self::ZERO
    }
}
