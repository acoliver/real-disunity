//! The runtime driven through its native API only: register a WGSL program, make a material,
//! set properties and features by name, and draw it. No Unity record exists in these suites and
//! they run without the `unity` feature.

mod common;
mod draw;
