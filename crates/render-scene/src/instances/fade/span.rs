//! Outward f32 bounds for the stock WGSL fade arithmetic. Intermediate rounding,
//! fused multiply-add and dot-product reassociation must not turn an empty CPU
//! result into a surviving GPU fragment. Ambiguous boundaries stay submitted.

#[derive(Clone, Copy, Debug)]
pub(super) struct Span {
    pub lo: f32,
    pub hi: f32,
}

impl Span {
    pub fn point(value: f32) -> Self {
        Self {
            lo: value,
            hi: value,
        }
    }

    fn rounded(lo: f32, hi: f32) -> Self {
        Self {
            lo: lo.next_down().min(if lo.abs() < f32::MIN_POSITIVE {
                0.0
            } else {
                lo
            }),
            hi: hi.next_up().max(if hi.abs() < f32::MIN_POSITIVE {
                0.0
            } else {
                hi
            }),
        }
    }

    /// `rounded` applied `steps` times. Each step moves a normal bound one unit in the last
    /// place, which stepping its bit pattern does across exponents, so bounds that stay normal
    /// move in one step; others take the loop.
    fn rounded_times(self, steps: u32) -> Self {
        if let (Some(lo), Some(hi)) = (
            stepped(self.lo, steps, false),
            stepped(self.hi, steps, true),
        ) {
            return Self { lo, hi };
        }
        let mut result = self;
        for _ in 0..steps {
            result = Self::rounded(result.lo, result.hi);
        }
        result
    }

    pub fn add(self, other: Self) -> Self {
        Self::rounded(self.lo + other.lo, self.hi + other.hi)
    }

    pub fn sub(self, other: Self) -> Self {
        Self::rounded(self.lo - other.hi, self.hi - other.lo)
    }

    pub fn mul(self, other: Self) -> Self {
        let values = [
            self.lo * other.lo,
            self.lo * other.hi,
            self.hi * other.lo,
            self.hi * other.hi,
        ];
        Self::rounded(
            values.into_iter().fold(f32::INFINITY, f32::min),
            values.into_iter().fold(f32::NEG_INFINITY, f32::max),
        )
    }

    pub fn div(self, other: Self) -> Self {
        // WGSL division permits 2.5 ULP error. Include reciprocal multiplication
        // and the change of ULP size across powers of two as well.
        self.mul(Self {
            lo: 1.0 / other.hi,
            hi: 1.0 / other.lo,
        })
        .rounded_times(8)
    }

    pub fn max(self, value: f32) -> Self {
        Self {
            lo: self.lo.max(value),
            hi: self.hi.max(value),
        }
    }

    pub fn maximum(self, other: Self) -> Self {
        Self {
            lo: self.lo.max(other.lo),
            hi: self.hi.max(other.hi),
        }
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }

    pub fn sum3([a, b, c]: [Self; 3]) -> Self {
        a.add(b)
            .add(c)
            .union(a.add(c).add(b))
            .union(b.add(c).add(a))
    }

    pub fn sum4(values: [Self; 4]) -> Self {
        let [a, b, c, d] = values;
        a.add(b)
            .add(c.add(d))
            .union(a.add(c).add(b.add(d)))
            .union(a.add(d).add(b.add(c)))
            .union(Self::sum3([a, b, c]).add(d))
            .union(Self::sum3([a, b, d]).add(c))
            .union(Self::sum3([a, c, d]).add(b))
            .union(Self::sum3([b, c, d]).add(a))
    }

    pub fn length(vector: [Self; 3]) -> Self {
        let squared = vector.map(|v| {
            let upper = v.lo.abs().max(v.hi.abs());
            let lower = if v.lo <= 0.0 && v.hi >= 0.0 {
                0.0
            } else {
                v.lo.abs().min(v.hi.abs())
            };
            Self::rounded(lower * lower, upper * upper).max(0.0)
        });
        let sum = Self::sum3(squared).max(0.0);
        // length inherits dot and sqrt precision; sqrt permits two ULP.
        Self {
            lo: sum.lo.sqrt(),
            hi: sum.hi.sqrt(),
        }
        .rounded_times(3)
        .max(0.0)
    }

    pub fn byte(self) -> Self {
        if self.lo.to_bits() == self.hi.to_bits()
            && (self.lo == 0.0 || self.lo.to_bits() == 1.0_f32.to_bits())
        {
            return self;
        }
        let scaled = self.mul(Self::point(255.0));
        let lower = scaled.lo.trunc();
        let upper = scaled.hi.trunc();
        // Both partition-byte's exact integer encoding and inclusive-byte's
        // floating division agree exactly at zero. Do not approximate R8 ties.
        if lower == 0.0 && upper == 0.0 {
            return Self::point(0.0);
        }
        if lower.to_bits() == 255.0_f32.to_bits() && upper.to_bits() == 255.0_f32.to_bits() {
            return Self::point(1.0);
        }
        Self {
            lo: lower,
            hi: upper,
        }
        .div(Self::point(255.0))
    }
}

/// A normal `value` moved `steps` units in the last place, up or down, when every step stays
/// normal; `rounded` then moves it exactly this way.
fn stepped(value: f32, steps: u32, up: bool) -> Option<f32> {
    if !value.is_normal() {
        return None;
    }
    let bits = value.to_bits();
    // For either sign, a larger bit pattern has a larger magnitude.
    let bits = if up == (value > 0.0) {
        bits.checked_add(steps)?
    } else {
        bits.checked_sub(steps)?
    };
    let moved = f32::from_bits(bits);
    (moved.is_normal() && moved.is_sign_negative() == value.is_sign_negative()).then_some(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepped_rounding_equals_the_rounding_loop_bit_for_bit() {
        let mut values = vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            f32::MIN_POSITIVE * 2.0,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            1.0e-40,
            -1.0e-40,
            2.0_f32.next_down(),
            -(2.0_f32.next_down()),
        ];
        let mut bits = 0x1234_5678_u32;
        for _ in 0..20_000 {
            bits ^= bits << 13;
            bits ^= bits >> 17;
            bits ^= bits << 5;
            values.push(f32::from_bits(bits));
        }
        for &lo in &values {
            for hi in [lo, -lo, 1.0, f32::MIN_POSITIVE, -3.5e-38] {
                for steps in [1, 3, 8] {
                    let mut looped = Span { lo, hi };
                    for _ in 0..steps {
                        looped = Span::rounded(looped.lo, looped.hi);
                    }
                    let fast = Span { lo, hi }.rounded_times(steps);
                    assert_eq!(
                        [fast.lo.to_bits(), fast.hi.to_bits()],
                        [looped.lo.to_bits(), looped.hi.to_bits()],
                        "{lo:e} {hi:e} {steps}"
                    );
                }
            }
        }
    }
}
