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
        let mut result = self.mul(Self {
            lo: 1.0 / other.hi,
            hi: 1.0 / other.lo,
        });
        // WGSL division permits 2.5 ULP error. Include reciprocal multiplication
        // and the change of ULP size across powers of two as well.
        for _ in 0..8 {
            result = Self::rounded(result.lo, result.hi);
        }
        result
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
        let mut result = Self {
            lo: sum.lo.sqrt(),
            hi: sum.hi.sqrt(),
        };
        // length inherits dot and sqrt precision; sqrt permits two ULP.
        for _ in 0..3 {
            result = Self::rounded(result.lo, result.hi);
        }
        result.max(0.0)
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
