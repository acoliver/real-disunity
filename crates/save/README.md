# Native application storage

`worldrunner-save` owns bounded file reads, atomic replacement, application slots,
settings and WorldDocument saves. Native applications use it through
`worldrunner::world::worldrunner_save`. Applications provide a directory and
`StoreLimits`; the store does not inspect the environment or choose payload schemas.

## Slots and settings

`ApplicationStore::new` validates positive limits without touching the filesystem.
Writes create the directory on demand. Each `SlotId` contains 1 to 64 lowercase
ASCII letters, digits, underscores or hyphens. Identifiers are case-sensitive;
uppercase letters, dots, separators, absolute paths and empty identifiers fail.
Files are named `slot-<id>.rds`, avoiding platform-reserved device filenames.

`write_slot` replaces one slot. `read_slot` returns payload bytes and `SlotMetadata`
(label and caller-owned payload version). `delete_slot` removes it. `list_slots`
returns identifiers in lexical order with payload sizes, filesystem modification
times and metadata. It validates each slot, including its payload digest, and fails
if any matching slot is invalid. An absent directory lists as empty. Other missing
files report `SaveError::Io` with `ErrorKind::NotFound`. The store never replaces
missing or corrupt settings with defaults.

Settings use `BTreeMap<String, SettingValue>` and support bool, i64, finite f64 and
UTF-8 strings. Keys must be nonempty. The value-byte limit applies to keys and
encoded values; bools occupy one byte and numbers eight bytes. The document-byte
limit includes keys and framing. Entry counts, key/value sizes, metadata labels
and slot payload sizes are bounded separately. Size checks precede encoding;
decoders check lengths before allocating strings. Duplicate keys, unknown value
tags, non-finite floats and malformed text fail explicitly. Settings are stored in
`settings.rds` through the same byte service as slots and world saves.

The application directory is caller-owned and trusted. This is not a sandbox
against another process modifying that directory, installing symlinks or racing
operations. Callers serialize writes and deletes. Listing validates one bounded
payload at a time but returns metadata for every slot; it has no aggregate slot
count limit.

## Envelope and replacement

Application files start with an 88-byte header: eight magic bytes (`RDAPP001`), a
little-endian u32 format version (1), a u32 metadata length, a u64 payload length,
a 32-byte BLAKE3 body digest and a 32-byte BLAKE3 header digest. Metadata contains
a u32 payload version followed by the UTF-8 label. Settings use payload version 1
for their engine-owned key/value encoding. Applications own slot payload versions.
Digests detect accidental corruption; they do not authenticate untrusted files.

`ByteFile` reads a fixed-size header before checking file size or allocating the
body. Application header checks reject unknown versions and oversize declarations
before body reads. Physical file size is checked as well, and reads are capped to
handle a file growing during the read. Truncation, length mismatches, header/body
corruption and unsupported versions have distinct `SaveError` variants.

Replacement uses `atomic-write-file`: stage in the destination directory, write,
sync, validate the staged bytes, then commit with atomic replacement. A failure
before commit leaves the previous destination intact. On Unix the parent directory
is synced after commit; a sync failure is reported even though replacement has
already happened. Windows uses the dependency's replacement semantics without a
parent-directory fsync. OS permission and read-only failures remain typed
`SaveError::Io` values with the original `std::io::Error` and its kind.

Application slots/settings keep no backup and perform no recovery fallback.
WorldDocument APIs keep their existing format, previous-world backup validation
and explicit `RecoveredWorld::source` (`Primary` or `Backup`). Both use `ByteFile`.

## Application directories

The desktop host exposes `worldrunner::scene::viewer::application_root` with the
`scene-viewer` feature. It selects a path without creating it: macOS uses
`$HOME/Library/Application Support`, Linux uses `$XDG_DATA_HOME` or
`$HOME/.local/share`, and Windows uses `%APPDATA%`. Missing required variables and
unsupported platforms return `ApplicationPathError`.

Application identifiers contain 1 to 64 bytes of lowercase ASCII letters, digits,
`.`, `-` or `_`. They must start with a letter or digit, contain no `..` sequence and
not end in `.`. Other identifiers return `ApplicationPathError::InvalidIdentifier`.
The identifier itself names the directory, for example
`$HOME/Library/Application Support/org.acoliver.chess3d`, so players can find their
saves. Lowercase-only identifiers avoid collisions on case-insensitive filesystems.
Application-storage tests use synthetic environment inputs and directories under
`target/`; they do not write user application data. Managed `PlayerPrefs.xml` remains
independent in the configured managed persistent-data directory. Native storage
does not delegate managed preferences.
