# Generic world document v1

This crate defines a fresh engine-owned format, not an import/save-inventory format.
There are no historical document snapshots, asset-ID mapping migrations, or historical
canonical hash algorithms. `CURRENT_WORLD_SCHEMA` and the generator boundary are v1.
Old source documents must not be deserialized into this contract as a migration shortcut.
The save consumer uses distinct `WRSAVE01` magic, envelope version 1, and accepts only
this schema; former `RDSAVE01` envelopes are rejected even when they say schema 1.

## Spatial contract

- Positions are metres in one right-handed Cartesian frame: X east, Y elevation,
  Z south (negative north). `position_xyz_meters` maps named world coordinates
  to this basis. Positive rotations follow the right-hand rule.
- Rotation is an active local-to-world unit quaternion `[x, y, z, w]`. Finite
  components and squared length within `1e-10` of one are required. No normalization
  or source-specific calibration happens during validation or hashing.
- Uniform scale is dimensionless fixed point (`1_000_000` = 1x), and must be positive.
- `Absolute` placement uses the authored origin unchanged. `BoundsGrounded` uses
  the position as an anchor for the scaled asset's lower Y bound.
- Embedded terrain is a centred, row-major heightfield. Columns increase east,
  rows increase north (-Z); heights and sample spacing are explicit millimetres.
  Height sampling bilinearly interpolates in metres and clamps outside the grid.

Import adapters must perform their own external unit, axis, and rotation conversion.
Native grid layouts, row-direction calibrations, and external terrain layer inventories
are not part of the schema. Terrain remains fully usable without an importer.

## Application and content policy

Every instance requires an application-supplied `visible` boolean. Hidden instances
still require valid transforms. There are no plant phases, source audit admission rules,
legacy renderability exceptions, or decoder/readiness policies. Asset availability and
actual rendering belong to consumers. Instance IDs are caller-owned, strictly ascending,
and unique; they are not handles into a private inventory.

Every world requires a complete `CatalogBinding`. There is no `Default` implementation
or serde fallback selecting art sets, platform profiles, or quality tiers. Preferences
are nonempty ordered lists of unique, nonblank, control-free caller-supplied identifiers.
Generic procedural numeric defaults remain in `GenerationParameters`; they select no assets.

Raw environment probes, accepted probe-key vocabularies, provenance-specific lifecycle
rules, and private water-inventory shapes are intentionally absent. Applications needing
water or lighting can supply the independent generic render-plan scene geometry/effects;
this document does not duplicate that contract or convert private records for it.

## Validation, serialization, and hashing

All document fields are required. Structs reject unknown self-describing fields rather
than silently discarding old payloads. After deserialization, consumers must call
`WorldDocument::validate`; the save consumer additionally rejects trailing binary bytes.

The canonical hash is BLAKE3 with a length-prefixed `WORLDRUNNER-WORLD-V1` domain and
all fields encoded in declaration order. Integers are little endian; lengths are `u64`;
floats use IEEE-754 bits with negative zero normalized to positive zero. Enums use explicit
stable tags, booleans use 0/1, and sequences carry their lengths. String UTF-8 bytes are
length-prefixed. Collection order is significant, including catalog preference priority.
Quaternion components are hashed as authored; antipodal quaternions are not collapsed.
The hash validates the world first and never drops unsupported or invalid data.
