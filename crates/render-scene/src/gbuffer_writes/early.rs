//! Direct canonical metadata outputs of the early material producer.
use super::replace_once;
use crate::gbuffer::GBUFFER_FORMATS;
use std::fmt::Write;
use wgpu::TextureFormat;

const NORMAL_MARKER: &str = "const SCENE_EARLY_GBUFFER_NORMALS: bool = true;";
const MATERIAL_MARKER: &str = "const SCENE_EARLY_GBUFFER_MATERIAL: bool = true;";
const ENCODE_SIGNATURE: &str =
    "fn scene_gbuffer_encode(surface: SceneGbufferSurface, view_from_scene: mat3x3<f32>)";
const ENCODE_CALL: &str = "scene_gbuffer_encode(surface, view_from_scene)";

/// Ordered outputs of the depth-writing material pass. The canonical normal follows the
/// optional velocity, and the optional SSR material follows the normal. No receiver payload
/// attachments are allocated: Front reconstructs from G3 and depth in the lighting consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EarlyOutputs {
    pub velocity: bool,
    pub reflections: bool,
}

impl EarlyOutputs {
    /// Physical attachment order shared by declarations, admission and pipeline targets.
    pub(crate) fn formats(self) -> Vec<TextureFormat> {
        let mut formats = GBUFFER_FORMATS.to_vec();
        if self.velocity {
            formats.push(worldrunner_render_post::VELOCITY_FORMAT);
        }
        formats.push(TextureFormat::Rgba16Float);
        if self.reflections {
            formats.push(TextureFormat::Rgba16Float);
        }
        formats
    }

    pub(crate) fn normal_location(self) -> u32 {
        4 + u32::from(self.velocity)
    }

    /// Reads the explicit output configuration embedded by this producer's source composer.
    pub(crate) fn from_source(source: &str) -> Option<Self> {
        source.contains(NORMAL_MARKER).then(|| Self {
            velocity: source.contains(crate::velocity::MARKER),
            reflections: source.contains(MATERIAL_MARKER),
        })
    }

    /// Uses the same material, cutout, card, relief and velocity code as observation.
    pub(crate) fn sources(self) -> (String, String) {
        let (object, terrain) = super::sources(self.velocity);
        (self.source(&object, true), self.source(&terrain, false))
    }

    fn fields(self) -> String {
        let normal = self.normal_location();
        let mut fields = format!("\n    @location({normal}) normal: vec4<f32>,\n");
        if self.reflections {
            writeln!(fields, "    @location({}) material: vec4<f32>,", normal + 1)
                .expect("string formatting");
        }
        fields
    }

    fn carried(self, value: &str) -> String {
        let mut values = format!(", {value}.normal");
        if self.reflections {
            write!(values, ", {value}.material").expect("string formatting");
        }
        values
    }

    fn source(self, stock: &str, object: bool) -> String {
        let mut source = extend_struct(stock, "SceneGbufferTargets", &self.fields());
        if object {
            source = extend_struct(&source, "SceneGbufferReliefTargets", &self.fields());
            source = replace_once(
                &source,
                "SceneGbufferReliefTargets(targets.g0, targets.g1, targets.g2, targets.g3, depth)",
                &format!(
                    "SceneGbufferReliefTargets(targets.g0, targets.g1, targets.g2, targets.g3, depth{})",
                    self.carried("targets")
                ),
            );
            let direction = crate::scene_metadata::receiver::function_source(
                include_str!("../scene_metadata/geometry.wgsl"),
                "scene_metadata_direction",
            );
            source.push_str(direction);
        }
        source = self.encoder(&source, object);
        if self.velocity {
            source = self.velocity_source(&source, object);
        }
        writeln!(source, "\n{NORMAL_MARKER}").expect("string formatting");
        if self.reflections {
            source.push_str(MATERIAL_MARKER);
            source.push('\n');
        }
        source
    }

    fn encoder(self, source: &str, object: bool) -> String {
        let normal = if object {
            "scene_metadata_direction(view_normal)"
        } else {
            "normalize(view_normal)"
        };
        let mut values = format!("        vec4<f32>({normal}, 1.0),\n");
        let mut source = source.to_owned();
        if self.reflections {
            source = replace_once(
                &source,
                ENCODE_SIGNATURE,
                "fn scene_gbuffer_encode(surface: SceneGbufferSurface, view_from_scene: mat3x3<f32>, metalness: f32)",
            );
            let metal = if object {
                "evaluated.surface.properties.y"
            } else {
                "material.projected.properties.y"
            };
            source = replace_once(
                &source,
                ENCODE_CALL,
                &format!("scene_gbuffer_encode(surface, view_from_scene, {metal})"),
            );
            values.push_str("        vec4<f32>(scene_material_f0(surface.base_colour, metalness, DIELECTRIC_F0), surface.roughness),\n");
        }
        let anchor = "        scene_gbuffer_g3_encode(surface.geometric_normal, surface.flags),\n";
        replace_once(&source, anchor, &format!("{anchor}{values}"))
    }

    fn velocity_source(self, source: &str, object: bool) -> String {
        let source = extend_struct(source, "SceneVelocityOutput", &self.fields());
        let source = extend_struct(&source, "SceneVelocityReliefOutput", &self.fields());
        let values = self.carried("t");
        let residual = if object {
            "scene_object_velocity(input)"
        } else {
            "vec2<f32>(0.0)"
        };
        let constructor = if object {
            ""
        } else {
            "SceneVelocityOutput(t.g0, t.g1, t.g2, t.g3, "
        };
        let anchor = format!("{constructor}{residual});");
        assert_eq!(
            source.matches(&anchor).count(),
            if object {
                super::FORWARD_OBJECTS.len()
            } else {
                1
            }
        );
        source.replace(&anchor, &format!("{constructor}{residual}{values});"))
    }
}

fn extend_struct(source: &str, name: &str, fields: &str) -> String {
    let signature = format!("struct {name} {{");
    assert_eq!(source.matches(&signature).count(), 1, "one output {name}");
    let start = source.find(&signature).expect("output struct") + signature.len();
    let end = start + source[start..].find('}').expect("output struct end");
    format!("{}{fields}{}", &source[..end], &source[end..])
}
