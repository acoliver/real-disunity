//! Render textures a kernel binds as storage images: marked before the frame builds, they are
//! made with storage binding usage when the device can bind their format, and a kernel's writes
//! are in them for what is recorded after.

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::compute_kernel::kernel_stage;
use worldrunner_render_program::unity::offscreen::frame::OffscreenFrame;
use worldrunner_render_program::unity::runtime::{BoundResource, DispatchRequest};
use worldrunner_render_program::unity::synthetic::stage_spirv;
use worldrunner_unity_serialized::compute_shader::{
    Code, ComputeBinding, ComputeSlot, ComputeVariant,
};

use super::rig::{Rig, texture};
use crate::common::submit;

const R16_SFLOAT: u32 = 45;
const TARGET: TextureId = TextureId(10);
const IMAGE_SLOT: u32 = (1 << 31) | (1 << 25) | (1 << 23);

/// Writes texel (x, y) of an RGBA8 image as (16x + 1, 16y + 2, x + 4y, 255).
const PATTERN: &str = "
@group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;
@compute @workgroup_size(4, 4, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    textureStore(image, vec2<i32>(id.xy), vec4<f32>(
        f32(16u * id.x + 1u) / 255.0,
        f32(16u * id.y + 2u) / 255.0,
        f32(id.x + 4u * id.y) / 255.0,
        1.0));
}";

fn pattern_variant() -> ComputeVariant {
    let words = stage_spirv(PATTERN, "main", naga::ShaderStage::Compute);
    ComputeVariant {
        constant_buffers: Vec::new(),
        textures: Vec::new(),
        samplers: Vec::new(),
        buffers: Vec::new(),
        writable: vec![ComputeBinding {
            name: "_Image".to_owned(),
            alias: String::new(),
            slot: ComputeSlot(IMAGE_SLOT),
            sampler: None,
            dimension: Some(2),
            reserved: 0,
        }],
        code: Code(words.iter().flat_map(|word| word.to_le_bytes()).collect()),
        thread_group_size: [4, 4, 1],
        requirements: 0,
        keywords: Vec::new(),
        following: Vec::new(),
    }
}

fn frame_of(format: u32) -> OffscreenFrame {
    let mut spec = texture(TARGET.0, [4, 4]);
    spec.graphics_format = format;
    OffscreenFrame {
        render_textures: vec![spec],
        ..OffscreenFrame::default()
    }
}

/// Dispatches the pattern kernel into the render texture and reads the texture back.
fn paint(rig: &mut Rig) -> Result<Vec<u8>, worldrunner_render_program::error::ProgramError> {
    let stage = kernel_stage(&pattern_variant(), &[]).unwrap();
    let program = rig.runtime.compute_program(&stage).unwrap();
    let handle = rig.state.texture_handle(TARGET).unwrap();
    let bindings = [BoundResource::texture(0, 0, handle)];
    let (pending, index) = submit(&mut rig.runtime, |runtime, frame| {
        runtime.dispatch(
            frame,
            &DispatchRequest {
                program: &program,
                bindings: &bindings,
                groups: [1, 1, 1],
            },
        )?;
        runtime.read_texture(frame, handle)
    })?;
    Ok(pending.resolve(index).unwrap().bytes)
}

#[test]
fn a_render_texture_marked_for_storage_takes_a_kernels_writes() {
    let mut rig = Rig::new();
    rig.state.bind_as_storage([TARGET]);
    rig.build(&frame_of(8)).unwrap();
    assert_eq!(
        rig.state.texture_storage(TARGET),
        Some((wgpu::TextureFormat::Rgba8Unorm, true))
    );
    let bytes = paint(&mut rig).unwrap();
    for y in 0..4u8 {
        for x in 0..4u8 {
            let at = usize::from(y) * 16 + usize::from(x) * 4;
            assert_eq!(
                bytes[at..at + 4],
                [16 * x + 1, 16 * y + 2, x + 4 * y, 255],
                "texel ({x}, {y})"
            );
        }
    }
}

#[test]
fn a_render_texture_that_is_not_marked_is_refused_as_a_storage_image_by_name() {
    let mut rig = Rig::new();
    rig.build(&frame_of(8)).unwrap();
    assert_eq!(
        rig.state.texture_storage(TARGET),
        Some((wgpu::TextureFormat::Rgba8Unorm, false))
    );
    let error = paint(&mut rig).unwrap_err();
    assert!(
        matches!(
            error,
            worldrunner_render_program::error::ProgramError::StorageImage {
                set: 0,
                binding: 0,
                error: worldrunner_render_program::error::StorageImageError::Usage { .. }
            }
        ),
        "{error}"
    );
}

#[test]
fn a_format_the_device_cannot_bind_as_storage_is_made_without_storage_and_still_builds() {
    let mut rig = Rig::new();
    assert!(!rig.runtime.supports_storage_image(
        wgpu::TextureFormat::R16Float,
        wgpu::StorageTextureAccess::WriteOnly
    ));
    rig.state.bind_as_storage([TARGET]);
    rig.build(&frame_of(R16_SFLOAT)).unwrap();
    assert_eq!(
        rig.state.texture_storage(TARGET),
        Some((wgpu::TextureFormat::R16Float, false))
    );
}

#[test]
fn marking_a_texture_that_exists_makes_it_again_once_and_keeps_it_after() {
    let mut rig = Rig::new();
    let frame = frame_of(8);
    rig.build(&frame).unwrap();
    let before = rig.state.texture_handle(TARGET).unwrap();
    assert_eq!(rig.state.texture_storage(TARGET).map(|t| t.1), Some(false));

    rig.state.bind_as_storage([TARGET]);
    rig.build(&frame).unwrap();
    let made = rig.state.texture_handle(TARGET).unwrap();
    assert_ne!(made, before, "made again with storage binding");
    assert_eq!(rig.state.texture_storage(TARGET).map(|t| t.1), Some(true));

    rig.build(&frame).unwrap();
    assert_eq!(rig.state.texture_handle(TARGET), Some(made), "kept");
}

#[test]
fn storage_promotion_budgets_the_complete_color_and_depth_replacement_before_mutation() {
    use worldrunner_render_program::error::ProgramError;
    use worldrunner_render_program::unity::offscreen::OffscreenError;
    let budget = 220;
    let mut rig = Rig::with_budget(budget);
    let mut frame = frame_of(8);
    frame.render_textures[0].depth_bits = 32;
    rig.build(&frame).unwrap();
    let before = rig.state.texture_handle(TARGET).unwrap();
    // The empty frame keeps 32 bytes of default vertex inputs alongside the textures.
    assert_eq!(rig.runtime.live_bytes(), 160);
    assert_eq!(
        rig.state
            .new_render_texture_bytes(&rig.runtime, &frame.render_textures),
        0
    );
    rig.state.bind_as_storage([TARGET]);
    for _ in 0..2 {
        let needed = rig
            .state
            .new_render_texture_bytes(&rig.runtime, &frame.render_textures);
        assert_eq!(
            needed, 128,
            "preflight includes both replacement attachments"
        );
        assert!(rig.runtime.live_bytes() + needed > budget);
        let error = rig.build(&frame).err().unwrap();
        assert!(
            matches!(
                error,
                OffscreenError::Program(ProgramError::OverBudget {
                    required: 224,
                    budget: 220,
                })
            ),
            "{error}"
        );
        assert_eq!(rig.runtime.live_bytes(), 160);
        assert_eq!(rig.state.texture_handle(TARGET), Some(before));
        assert_eq!(
            rig.state.texture_storage(TARGET).map(|value| value.1),
            Some(false)
        );
    }
}

#[test]
fn storage_promotion_fits_the_exact_color_and_depth_replacement_budget() {
    let mut rig = Rig::with_budget(288);
    let mut frame = frame_of(8);
    frame.render_textures[0].depth_bits = 32;
    rig.build(&frame).unwrap();
    let before = rig.state.texture_handle(TARGET).unwrap();
    rig.state.bind_as_storage([TARGET]);
    let needed = rig
        .state
        .new_render_texture_bytes(&rig.runtime, &frame.render_textures);
    assert_eq!(needed, 128);
    assert_eq!(rig.runtime.live_bytes() + needed, rig.runtime.budget().0);
    rig.build(&frame).unwrap();
    let made = rig.state.texture_handle(TARGET).unwrap();
    assert_ne!(made, before);
    assert_eq!(
        rig.state.texture_storage(TARGET).map(|value| value.1),
        Some(true)
    );
    rig.runtime.collect();
    assert_eq!(rig.runtime.live_bytes(), 160);
    assert_eq!(
        rig.state
            .new_render_texture_bytes(&rig.runtime, &frame.render_textures),
        0
    );
    rig.build(&frame).unwrap();
    assert_eq!(rig.state.texture_handle(TARGET), Some(made));
    assert_eq!(rig.runtime.live_bytes(), 160);
}
