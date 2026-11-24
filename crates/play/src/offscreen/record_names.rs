//! How a record that does not build is named in the report: by its kind, and by the shader and
//! pass it draws with when it has a material.

use worldrunner_render_program::unity::offscreen::OffscreenState;
use worldrunner_render_program::unity::offscreen::frame::{OffscreenFrame, Record};

/// The kind of `record`, as the report names it.
pub(super) fn kind(record: &Record) -> &'static str {
    record.kind()
}

/// The shader and pass `record` draws with, as `shader NAME pass PASS`, when it names a material
/// whose shader the state holds.
pub(super) fn drawn_with(
    record: &Record,
    frame: &OffscreenFrame,
    state: &OffscreenState,
) -> Option<String> {
    let (material, pass) = match record {
        Record::Blit {
            material: Some(material),
            pass,
            ..
        } => (material, pass.to_string()),
        Record::Procedural(draw) => (&draw.material, format!("{:?}", draw.passes)),
        _ => return None,
    };
    let shader = frame.materials.get(material)?.shader?;
    Some(format!("shader {} pass {pass}", state.shader_name(shader)?))
}
