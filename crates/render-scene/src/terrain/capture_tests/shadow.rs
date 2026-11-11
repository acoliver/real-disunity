//! Shadow receivers: the terrain sources that sample the directional shadow map at set two,
//! in the contexts that carry one, with the shadow group of two frame slots.

use wgpu::{BindGroup, TextureFormat};

use super::{Case, Legacy, Rig, Source, Target, capture_against, drawable, scenes};
use crate::device::GpuContext;
use crate::dynamic::capture_tests::{
    HDR, VELOCITY, ambient_group, context, reflection_group, shadow_fixture,
};
use crate::renderer::ScenePipelines;
use crate::renderer::depth::DepthMode;

#[derive(Clone, Copy)]
enum Screen {
    None,
    Ambient,
    Reflection,
}

struct Set {
    name: &'static str,
    screen: Screen,
    velocity: bool,
    samples: u32,
}

const SETS: [Set; 6] = [
    Set {
        name: "shadowed-hdr-1x",
        screen: Screen::None,
        velocity: false,
        samples: 1,
    },
    Set {
        name: "shadowed-hdr-4x",
        screen: Screen::None,
        velocity: false,
        samples: 4,
    },
    Set {
        name: "shadowed-velocity-4x",
        screen: Screen::None,
        velocity: true,
        samples: 4,
    },
    Set {
        name: "shadowed-ssao-1x",
        screen: Screen::Ambient,
        velocity: false,
        samples: 1,
    },
    Set {
        name: "shadowed-ssao-velocity-4x",
        screen: Screen::Ambient,
        velocity: true,
        samples: 4,
    },
    Set {
        name: "shadowed-reflection-velocity-4x",
        screen: Screen::Reflection,
        velocity: true,
        samples: 4,
    },
];

impl Set {
    fn formats(&self) -> Vec<TextureFormat> {
        let mut formats = vec![
            HDR;
            if matches!(self.screen, Screen::Reflection) {
                3
            } else {
                1
            }
        ];
        if self.velocity {
            formats.push(VELOCITY);
        }
        formats
    }

    fn supported(&self, context: &GpuContext) -> bool {
        self.formats()
            .iter()
            .chain(&[TextureFormat::Depth32Float])
            .all(|&format| {
                context
                    .adapter()
                    .get_texture_format_features(format)
                    .flags
                    .sample_count_supported(self.samples)
            })
    }
}

/// The shadow map the receivers sample: four layers cleared to one depth, so the terrain's
/// height decides which fragments are shadowed.
pub(super) struct Shadow {
    pub groups: [BindGroup; 2],
}

impl Shadow {
    pub(super) fn new(context: &GpuContext) -> Self {
        Self {
            groups: shadow_fixture::groups(context),
        }
    }
}

/// Pipelines whose terrain and objects sample the shadow map, with the group they need at set
/// three.
fn pipelines(context: &GpuContext, set: &Set) -> (ScenePipelines, Option<BindGroup>) {
    let device = context.device();
    let layout = crate::shadows::water_layout(device);
    let (ssao, reflection) = (
        matches!(set.screen, Screen::Ambient),
        matches!(set.screen, Screen::Reflection),
    );
    let (object, terrain) = crate::shadows::receiver_sources(ssao, reflection, set.velocity);
    let (screen_layout, screen) = match set.screen {
        Screen::None => (None, None),
        Screen::Ambient => (
            Some(crate::ssao::ambient_layout(device)),
            Some(ambient_group(device)),
        ),
        Screen::Reflection => {
            let screen_layout = crate::ssr::forward::layout(device);
            let group = reflection_group(device, &screen_layout);
            (Some(screen_layout), Some(group))
        }
    };
    let mut built = ScenePipelines::with_screen_space(
        device,
        (HDR, set.samples),
        &object,
        &terrain,
        Some(&layout),
        screen_layout.as_ref(),
    );
    built.ssao = ssao;
    (built, screen)
}

fn shadowed(source: Source, mode: DepthMode) -> usize {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    if !drawable(&context, source) {
        return 0;
    }
    let shadow = Shadow::new(&context);
    let mut run = 0;
    for set in SETS
        .iter()
        .filter(|set| mode == DepthMode::Independent || set.samples == 1)
    {
        if !set.supported(&context) {
            println!(
                "TERRAIN-CAPTURE config={} depth={mode:?} source={} skipped: format or samples unsupported",
                set.name,
                source.name()
            );
            continue;
        }
        let (built, screen) = pipelines(&context, set);
        let formats = set.formats();
        let rig = Rig::new(
            &context,
            &built,
            &source.scene(),
            (source, scenes::frame(scenes::pose())),
        );
        let built = built.for_depth(mode);
        let legacy = Legacy::new(&context, &rig.gpu, &rig.selected(&built));
        for (slot, group) in shadow.groups.iter().enumerate() {
            let name = format!("{}-slot{slot}", set.name);
            capture_against(
                &rig,
                &Case {
                    name: &name,
                    source: source.name(),
                    target: Target {
                        formats: &formats,
                        samples: set.samples,
                    },
                    pipelines: &built,
                    screen: screen.as_ref(),
                    shadow: Some(group),
                },
                &legacy,
            );
            run += 1;
        }
    }
    println!(
        "TERRAIN-CAPTURE-SUMMARY set=shadow depth={mode:?} source={} compared={run}",
        source.name()
    );
    run
}

#[test]
fn shadow_receiving_plain_terrain_matches_the_fixed_pipeline_byte_for_byte() {
    assert!(shadowed(Source::Plain, DepthMode::Independent) > 0);
}

#[test]
fn shadow_receiving_page_terrain_matches_the_fixed_pipeline_byte_for_byte() {
    assert!(shadowed(Source::Pages, DepthMode::Independent) > 0);
}

#[test]
fn shadow_receiving_layered_terrain_matches_the_fixed_pipeline_byte_for_byte() {
    shadowed(Source::Layered, DepthMode::Independent);
}

#[test]
fn shadow_receiving_layered_page_terrain_matches_the_fixed_pipeline_byte_for_byte() {
    shadowed(Source::LayeredPages, DepthMode::Independent);
}

#[test]
fn shadow_receiving_plain_terrain_matches_the_fixed_pipeline_byte_for_byte_with_depth_reuse() {
    assert!(shadowed(Source::Plain, DepthMode::Reuse) > 0);
}

#[test]
fn shadow_receiving_page_terrain_matches_the_fixed_pipeline_byte_for_byte_with_depth_reuse() {
    assert!(shadowed(Source::Pages, DepthMode::Reuse) > 0);
}

#[test]
fn shadow_receiving_layered_terrain_matches_the_fixed_pipeline_byte_for_byte_with_depth_reuse() {
    shadowed(Source::Layered, DepthMode::Reuse);
}

#[test]
fn shadow_receiving_layered_page_terrain_matches_the_fixed_pipeline_byte_for_byte_with_depth_reuse()
{
    shadowed(Source::LayeredPages, DepthMode::Reuse);
}
