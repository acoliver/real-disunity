# Portable generation contract

Generator revision 2 retains schema version 1, the authored profile contract, and
revision-1 hash streams. It intentionally replaces only the generated rotation
algorithm. Existing saved documents remain readable; regenerating a revision-1
request now records generator revision 2 and produces a new canonical hash.
Revision-1 platform-specific rotation bits are not reproduced.

## Cause

At revision `cffde121`, terrain, climate, candidate locations, prop selection,
scales, and biome classification already used deterministic integer operations.
Generated rotation was the exception: `EnvironmentOrientation::from_yaw_radians`
called `f64::sin_cos`. Rust does not promise platform-independent precision for
that operation. Canonical hashing includes every quaternion component's exact
IEEE-754 bits (apart from signed-zero normalization), so a last-bit libm
variation changes the world hash. Repeating generation on one machine cannot
expose that problem. The reported macOS hash was `558d7c9a57e58174e088c603acec0c85243ab2ad654a61e595d85f0560edd0fb`,
whereas the original Linux golden was `4ff0e0ab57f8a7e1b5de5a964facc50e0a0215ab021db9198525c7c9f9d4f98a`.

The schema's general radians constructor remains unchanged; it is not a
bit-portable generation API. Hashing is not weakened to mask numeric differences.

## Revision-2 rotation algorithm

1. Retain `yaw = environment_kind_draw % 6_283_185`, in microradians.
2. Compute the half-angle in Q56 by integer division:
   `x = yaw * 2^56 / 2_000_000`. Compute `x_squared = x*x / 2^56`.
3. Starting with sine term/sum `x` and cosine term/sum `2^56`, perform iterations
   `n = 1..=16`. Each sine term is `-term*x_squared / 2^56 / ((2*n)*(2*n+1))`;
   each cosine term is `-term*x_squared / 2^56 / ((2*n-1)*(2*n))`.
   Add each term to its respective sum. Every division truncates toward zero;
   intermediate signed i128 products stay below `2^120`.
4. Round each sum from Q56 to Q40, nearest with ties away from zero:
   `(sum + sign(sum)*2^15) / 2^16`.
5. Convert the bounded Q40 integer numerator exactly to f64, then divide by
   `2^40` exactly. Emit `[+0, sine, +0, cosine]`, without host normalization.

This is a specified integer approximation, not a promise of correctly rounded
mathematical sine/cosine. The Taylor remainder on `[0, pi)` is below `4e-22`;
fixed-point rounding dominates that remainder but remains far below the Q40
quantum. Q40 output introduces at most approximately `4.55e-13` component error,
comfortably within the schema's `1e-10` squared-length tolerance. Tests check
numeric vectors independently calculated with 85-digit decimal arithmetic,
exact output bits, wraparound, and accuracy/unit length across the full turn.
No runtime transcendental, square root, or platform-dependent normalization is
used in generation. Remaining integer-to-metre conversions use exactly
representable i32 values followed by IEEE-754 division by 1000.

## Behavioral signatures

For development seed 42 with `tests/fixtures/generation-profile-v1.json`:

- Terrain heights (i32 LE), followed by material indices (u16 LE), BLAKE3:
  `2af986c74c7abd461c0b0558decbb8468e462b9174a1eb730625e4c36f7437c3`.
  Captured before the change; unchanged in revision 2.
- For each generated instance in order: instance ID (u64 LE), followed by four
  quaternion f64 bit patterns (u64 LE), BLAKE3:
  `34662a66d2047d461c11703694b806e8a3f630589ff450e91129ab6234b57bdb`.
- Full canonical world hash, including generator revision 2:
  `dea43c1c90bf9095183ed85bf6669c677ccedb83d76a53d1285f0ecd612ea104`.

These are single platform-independent expectations, not per-platform goldens.
The full-world golden changes deliberately because the algorithm and recorded
generator revision changed, not because either previous host hash is preferred.
