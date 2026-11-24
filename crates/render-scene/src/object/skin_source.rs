//! The skinned form of an object program's source.
//!
//! The static source is specialized by text, the way the velocity rewrite does it: each anchor
//! must match exactly as often as expected, so a change to the stock vertex stage fails here
//! with the anchor named. The static form returns its input, which keeps every composed static
//! source byte for byte what it was.

const SKINNING: &str = include_str!("../shader/skinning.wgsl");
const SKIN_VERTEX: &str = include_str!("../shader/skin_vertex.wgsl");

/// Binding of the palette in the material group, a vertex-stage read-only storage buffer.
pub(crate) const PALETTE_BINDING: u32 = 11;

const TRANSFORM_ANCHOR: &str = "fn transform_vertex(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {\n    let model = ";
const TRANSFORM_SKINNED: &str = "fn transform_vertex(source: VertexInput, instance: InstanceInput) -> VertexOutput {\n    let vertex = scene_skin_local(source);\n    let model = ";
const ENTRY_ANCHOR: &str = "@vertex fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {\n    return material_vertex(vertex,instance);";
const ENTRY_SKINNED: &str = "@vertex fn vs_main(vertex: VertexInput, instance: InstanceInput, skin: SceneSkinInput) -> VertexOutput {\n    scene_skin_input = skin;\n    return material_vertex(vertex,instance);";

const CASTER_ANCHOR: &str = "@vertex\nfn vs_shadow(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {\n    return material_vertex_pass(vertex, instance, false);";
const CASTER_SKINNED: &str = "@vertex\nfn vs_shadow(vertex: VertexInput, instance: InstanceInput, skin: SceneSkinInput) -> VertexOutput {\n    scene_skin_input = skin;\n    return material_vertex_pass(vertex, instance, false);";

fn replace_once(source: &str, from: &str, to: &str) -> String {
    assert_eq!(
        source.matches(from).count(),
        1,
        "skin rewrite target changed: {from}"
    );
    source.replace(from, to)
}

/// The object source with its vertex stage deformed by the scene palette.
///
/// `source` is a finished non-velocity object source: the previous-pose form of the vertex
/// stage rewrites `vs_main` and is not combined with this one.
pub(crate) fn skinned_source(source: &str) -> String {
    let source = replace_once(source, TRANSFORM_ANCHOR, TRANSFORM_SKINNED);
    let source = replace_once(&source, ENTRY_ANCHOR, ENTRY_SKINNED);
    // Caster sources add the shadow-map entry, which places geometry through the same
    // transform_vertex and so needs only the skin input.
    let source = if source.contains("fn vs_shadow(") {
        replace_once(&source, CASTER_ANCHOR, CASTER_SKINNED)
    } else {
        source
    };
    format!("{source}\n{SKINNING}\n{SKIN_VERTEX}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(source: &str) {
        let module = naga::front::wgsl::parse_str(source).expect("the skinned source parses");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the skinned module validates as a whole");
    }

    /// The object sources the forward, receiver and screen-space contexts
    /// compose, without the previous-pose rewrite that skinned geometry does not take yet.
    fn contexts() -> Vec<(&'static str, String)> {
        let mut sources = vec![
            ("forward", crate::shader::ENVIRONMENT_WGSL.to_owned()),
            (
                "ambient gather",
                crate::ssao::ambient_source(crate::shader::ENVIRONMENT_WGSL),
            ),
            (
                "reflection",
                crate::ssr::forward::source(crate::shader::ENVIRONMENT_WGSL, false),
            ),
        ];
        sources.push(("shadow caster", crate::shadows::caster_source()));
        sources.push((
            "metadata prepass",
            crate::scene_metadata::sources(crate::scene_metadata::MetadataOutput::Normal).0,
        ));
        for (ssao, reflection) in [(false, false), (true, false), (false, true), (true, true)] {
            sources.push((
                "shadow receiver",
                crate::shadows::receiver_sources(ssao, reflection, false).0,
            ));
        }
        sources
    }

    #[test]
    fn every_pass_context_composes_a_valid_skinned_module() {
        for (name, source) in contexts() {
            let skinned = skinned_source(&source);
            validate(&skinned);
            assert!(
                skinned.contains("scene_skin_local(source)"),
                "{name} deforms its vertex in transform_vertex"
            );
            let entries = 1 + usize::from(source.contains("fn vs_shadow("));
            assert_eq!(
                skinned.matches("scene_skin_input = skin;").count(),
                entries,
                "{name}: every placing vertex entry reads the skin input"
            );
        }
    }

    /// The vertex locations bound by `ty`, a struct of located members or a located value.
    fn collect(
        module: &naga::Module,
        ty: naga::Handle<naga::Type>,
        binding: Option<&naga::Binding>,
        found: &mut Vec<u32>,
    ) {
        if let Some(naga::Binding::Location { location, .. }) = binding {
            found.push(*location);
        }
        if let naga::TypeInner::Struct { members, .. } = &module.types[ty].inner {
            for member in members {
                collect(module, member.ty, member.binding.as_ref(), found);
            }
        }
    }

    #[test]
    fn the_unlit_entry_point_keeps_its_own_location_nine() {
        let skinned = skinned_source(crate::shader::ENVIRONMENT_WGSL);
        let module = naga::front::wgsl::parse_str(&skinned).expect("parses");
        let locations = |name: &str| -> Vec<u32> {
            let entry = module
                .entry_points
                .iter()
                .find(|entry| entry.name == name)
                .unwrap_or_else(|| panic!("{name} is an entry point"));
            let mut found = Vec::new();
            for argument in &entry.function.arguments {
                collect(&module, argument.ty, argument.binding.as_ref(), &mut found);
            }
            found
        };
        let main = locations("vs_main");
        assert_eq!(main.iter().filter(|&&at| at == 9).count(), 1);
        assert!(main.contains(&15));
        let unlit = locations("vs_unlit");
        assert_eq!(unlit.iter().filter(|&&at| at == 9).count(), 1);
        assert!(!unlit.contains(&15));
    }
}
