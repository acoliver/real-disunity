//! IR bounds for repeated relief reconstruction and blended atlas sampling.
use naga::{Block, Expression, Function, Module, SampleLevel, Statement};

use super::geometry_test_sources::geometry_sources;

fn relief_calls(module: &Module, block: &Block) -> usize {
    block
        .iter()
        .map(|statement| match statement {
            Statement::Call { function, .. } => {
                let callee = &module.functions[*function];
                usize::from(callee.name.as_deref() == Some("scene_relief_position"))
                    + relief_calls(module, &callee.body)
            }
            Statement::If { accept, reject, .. } => {
                relief_calls(module, accept) + relief_calls(module, reject)
            }
            Statement::Block(block) => relief_calls(module, block),
            Statement::Loop {
                body, continuing, ..
            } => relief_calls(module, body) + relief_calls(module, continuing),
            Statement::Switch { cases, .. } => cases
                .iter()
                .map(|case| relief_calls(module, &case.body))
                .sum(),
            _ => 0,
        })
        .sum()
}

fn camera_sources() -> Vec<String> {
    let sources = geometry_sources();
    let base = sources[1]
        .split_once(&crate::scene_metadata::single_fragment(
            "vec4<f32>(normal, 1.0)",
        ))
        .unwrap()
        .0;
    let merged = format!(
        "{base}\n{}\n{}",
        include_str!("../ssao/camera.wgsl"),
        crate::scene_metadata::MetadataOutput::Merged.object_fragment()
    );
    let paired = [
        crate::scene_metadata::receiver::ReceiverLayout::Front,
        crate::scene_metadata::receiver::ReceiverLayout::Opposing,
    ]
    .into_iter()
    .map(|layout| {
        let source = crate::shadows::receiver_sources(true, true, true).0;
        crate::scene_metadata::receiver::consumer_source(&source, layout, false)
    });
    sources
        .into_iter()
        .take(3)
        .chain([merged])
        .chain(paired)
        .collect()
}

#[test]
fn relief_entry_points_reconstruct_position_once() {
    let mut checked = 0;
    for source in camera_sources() {
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        for entry in module
            .entry_points
            .iter()
            .filter(|entry| matches!(entry.name.as_str(), "fs_relief_opaque" | "fs_relief_masked"))
        {
            assert_eq!(
                relief_calls(&module, &entry.function.body),
                1,
                "{} must share relief reconstruction between depth and shading",
                entry.name
            );
            checked += 1;
        }
    }
    // Eight original entries plus two relief entries in each receiver layout.
    assert_eq!(checked, 12);
}

fn gradient_samples(function: &Function, block: &Block) -> usize {
    block
        .iter()
        .map(|statement| match statement {
            Statement::Emit(range) => range
                .clone()
                .filter(|handle| {
                    matches!(
                        function.expressions[*handle],
                        Expression::ImageSample {
                            level: SampleLevel::Gradient { .. },
                            ..
                        }
                    )
                })
                .count(),
            _ => 0,
        })
        .sum()
}

fn exclusive_level_samples(function: &Function, block: &Block, exclusive: bool) -> usize {
    let mut count = 0;
    for statement in block {
        match statement {
            Statement::Emit(range) => {
                for handle in range.clone() {
                    if matches!(
                        function.expressions[handle],
                        Expression::ImageSample {
                            level: SampleLevel::Exact(_),
                            ..
                        }
                    ) {
                        assert!(exclusive, "Level sample must exclude the Grad branch");
                        count += 1;
                    }
                }
            }
            Statement::If { accept, reject, .. } => {
                count += exclusive_level_samples(function, accept, exclusive);
                count += exclusive_level_samples(
                    function,
                    reject,
                    exclusive || gradient_samples(function, accept) == 1,
                );
            }
            Statement::Block(block) => {
                count += exclusive_level_samples(function, block, exclusive);
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                count += exclusive_level_samples(function, body, exclusive);
                count += exclusive_level_samples(function, continuing, exclusive);
            }
            _ => {}
        }
    }
    count
}

#[test]
fn blended_atlas_level_samples_are_exclusive_with_gradients() {
    for source in geometry_sources() {
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        for name in ["scene_atlas_sample", "scene_relief_normal"] {
            let function = module
                .functions
                .iter()
                .find(|(_, function)| function.name.as_deref() == Some(name))
                .unwrap()
                .1;
            assert_eq!(exclusive_level_samples(function, &function.body, false), 1);
        }
    }
}
