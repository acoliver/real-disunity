//! Shadow receivers: the object sources that sample the directional shadow map at set two, in
//! the contexts that carry one, with the shadow group of two frame slots.

use wgpu::{BindGroup, TextureFormat};

use super::{Case, Target, capture_scenes};
use crate::device::GpuContext;
use crate::dynamic::capture_tests::shadow_fixture::groups;
use crate::dynamic::capture_tests::{HDR, VELOCITY, ambient_group, context, reflection_group};
use crate::pipeline::PipelineKey;
use crate::renderer::ScenePipelines;

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

/// Pipelines whose objects sample the shadow map, with the group they need at set three.
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
        crate::renderer::SceneTarget {
            format: HDR,
            samples: set.samples,
        },
        &object,
        &terrain,
        Some(&layout),
        screen_layout.as_ref(),
    );
    built.ssao = ssao;
    (built, screen)
}

#[test]
fn shadow_receiving_objects_draw_in_every_shadow_slot() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let shadow = groups(&context);
    let (mut run, mut skipped) = (0, 0);
    for set in &SETS {
        if !set.supported(&context) {
            println!(
                "OBJECT-CAPTURE config={} skipped: format or samples unsupported",
                set.name
            );
            skipped += 1;
            continue;
        }
        let (built, screen) = pipelines(&context, set);
        let formats = set.formats();
        for (slot, group) in shadow.iter().enumerate() {
            let name = format!("{}-slot{slot}", set.name);
            let (keys, compared) = capture_scenes(
                &context,
                &Case {
                    name: &name,
                    scene: "",
                    target: Target {
                        formats: &formats,
                        samples: set.samples,
                    },
                    pipelines: &built,
                    screen: screen.as_ref(),
                    shadow: Some(group),
                    mask: 0,
                },
                None,
            );
            assert_eq!(
                keys.len(),
                PipelineKey::ALL.len(),
                "{name}: every key is drawn"
            );
            run += compared;
        }
    }
    println!("OBJECT-CAPTURE-SUMMARY set=shadow captures={run} skipped={skipped}");
    assert_eq!(skipped, 0, "a skipped capture fails");
}
