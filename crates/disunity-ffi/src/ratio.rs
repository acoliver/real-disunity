use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

pub(crate) const MAX_NUMERATOR: u64 = 9_007_199_254_740_991;
pub(crate) const MAX_DENOMINATOR: u64 = 1_000_000_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ratio {
    pub num: u64,
    pub den: u64,
}
impl Default for Ratio {
    fn default() -> Self {
        Self::ZERO
    }
}
impl Ratio {
    pub const ZERO: Self = Self { num: 0, den: 1 };
    pub const ONE: Self = Self { num: 1, den: 1 };
    pub const FIXED: Self = Self { num: 1, den: 50 };

    pub fn checked(self) -> Result<Self, String> {
        if self.num > MAX_NUMERATOR || self.den == 0 || self.den > MAX_DENOMINATOR {
            return Err("rational component outside supported bounds".into());
        }
        Self::reduce(u128::from(self.num), u128::from(self.den))
    }
    fn reduce(num: u128, den: u128) -> Result<Self, String> {
        let divisor = gcd(num, den);
        let (num, den) = (num / divisor, den / divisor);
        if num > u128::from(MAX_NUMERATOR) || den > u128::from(MAX_DENOMINATOR) {
            return Err("exact rational arithmetic exceeds supported bounds".into());
        }
        Ok(Self {
            num: num as u64,
            den: den as u64,
        })
    }
    pub fn add(self, other: Self) -> Result<Self, String> {
        Self::reduce(
            u128::from(self.num) * u128::from(other.den)
                + u128::from(other.num) * u128::from(self.den),
            u128::from(self.den) * u128::from(other.den),
        )
    }
    pub fn sub(self, other: Self) -> Result<Self, String> {
        let left = u128::from(self.num) * u128::from(other.den);
        let right = u128::from(other.num) * u128::from(self.den);
        Self::reduce(
            left.checked_sub(right)
                .ok_or("rational time moved backwards")?,
            u128::from(self.den) * u128::from(other.den),
        )
    }
    pub fn mul(self, other: Self) -> Result<Self, String> {
        Self::reduce(
            u128::from(self.num) * u128::from(other.num),
            u128::from(self.den) * u128::from(other.den),
        )
    }
    pub fn steps(self, fixed: Self) -> u128 {
        (u128::from(self.num) * u128::from(fixed.den))
            / (u128::from(self.den) * u128::from(fixed.num))
    }
    pub fn float(self) -> f64 {
        self.num as f64 / self.den as f64
    }
}
impl PartialOrd for Ratio {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Ratio {
    fn cmp(&self, other: &Self) -> Ordering {
        (u128::from(self.num) * u128::from(other.den))
            .cmp(&(u128::from(other.num) * u128::from(self.den)))
    }
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
