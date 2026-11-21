//! A declaration's mip chain: the subresources it names and the counts the configuration
//! refuses, by the target's name, before anything is allocated.
use super::{TargetDeclaration, TargetExtent};
use worldrunner_render_post::{PostError, ResourceRef};

const BC4: wgpu::TextureFormat = wgpu::TextureFormat::Bc4RUnorm;

fn validate(declaration: &TargetDeclaration, frame: [u32; 2]) -> Result<(), String> {
    declaration
        .validate(
            (
                &wgpu::Limits::default(),
                wgpu::Features::TEXTURE_COMPRESSION_BC,
            ),
            frame,
        )
        .map_err(|error| match error {
            PostError::Allocation(reason) => reason,
            other => panic!("expected an allocation refusal, got {other:?}"),
        })
}

#[test]
fn a_declaration_has_one_mip_unless_it_asks_for_a_chain() {
    let plain = TargetDeclaration::new("chain", BC4, TargetExtent::Fixed([8, 8]));
    assert_eq!(plain.mips, 1);
    let chain = plain.clone().mips(3).layers(2);
    assert_eq!(chain.mips, 3);
    assert_eq!(
        chain.mip(1, 2),
        ResourceRef {
            name: "chain",
            version: 0,
            mip: 2,
            layer: 1,
        }
    );
    assert_eq!(chain.layer(1), chain.mip(1, 0));
    assert_eq!(
        chain.layer_mips(1).collect::<Vec<_>>(),
        [chain.mip(1, 0), chain.mip(1, 1), chain.mip(1, 2)]
    );
    assert_eq!(plain.layer_mips(0).collect::<Vec<_>>(), [plain.layer(0)]);
}

#[test]
fn a_block_chain_may_reach_one_texel_through_rounded_up_blocks() {
    // 8, 4, 2 and 1 texels a side: the last two are one block each, rounded up as wgpu sizes them.
    let full = TargetDeclaration::new("chain", BC4, TargetExtent::Fixed([8, 8])).mips(4);
    assert_eq!(validate(&full, [64, 48]), Ok(()));
    let wide = TargetDeclaration::new("chain", BC4, TargetExtent::Fixed([16, 4])).mips(5);
    assert_eq!(validate(&wide, [64, 48]), Ok(()));
}

#[test]
fn a_mip_count_outside_the_chain_is_refused_by_name() {
    for (mips, extent, most) in [(0, [8, 8], 4), (5, [8, 8], 4), (6, [16, 4], 5)] {
        let declaration =
            TargetDeclaration::new("chain", BC4, TargetExtent::Fixed(extent)).mips(mips);
        let reason = validate(&declaration, [64, 48]).unwrap_err();
        assert_eq!(
            reason,
            format!(
                "game target chain: mip count {mips} is outside 1..={most} for the extent {}x{}",
                extent[0], extent[1]
            )
        );
    }
}

#[test]
fn a_frame_sized_chain_is_checked_against_the_frame_it_resolves_to() {
    let declaration =
        TargetDeclaration::new("chain", wgpu::TextureFormat::Bc4RUnorm, TargetExtent::Frame)
            .mips(4);
    assert_eq!(validate(&declaration, [16, 16]), Ok(()));
    assert_eq!(
        validate(&declaration, [4, 4]).unwrap_err(),
        "game target chain: mip count 4 is outside 1..=3 for the extent 4x4"
    );
}

#[test]
fn a_renderable_target_may_carry_a_chain_to_one_texel() {
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureFormat::R32Float,
    ] {
        let full = TargetDeclaration::new("painted", format, TargetExtent::Fixed([16, 4])).mips(5);
        assert_eq!(validate(&full, [64, 48]), Ok(()), "{format:?}");
        let beyond =
            TargetDeclaration::new("painted", format, TargetExtent::Fixed([16, 4])).mips(6);
        assert_eq!(
            validate(&beyond, [64, 48]).unwrap_err(),
            "game target painted: mip count 6 is outside 1..=5 for the extent 16x4",
            "{format:?}"
        );
    }
    let frame_sized = TargetDeclaration::new(
        "painted",
        wgpu::TextureFormat::Rgba16Float,
        TargetExtent::Frame,
    )
    .mips(6);
    assert_eq!(validate(&frame_sized, [64, 48]), Ok(()));
    assert!(validate(&frame_sized, [16, 16]).is_err());
}

#[test]
fn a_chain_on_a_format_that_cannot_render_is_refused_by_name() {
    for format in [
        wgpu::TextureFormat::Rg11b10Ufloat,
        wgpu::TextureFormat::Depth32Float,
    ] {
        let declaration =
            TargetDeclaration::new("painted", format, TargetExtent::Fixed([8, 8])).mips(2);
        assert_eq!(
            validate(&declaration, [64, 48]).unwrap_err(),
            "game target painted: format is not a renderable colour format",
            "{format:?}"
        );
    }
}

#[test]
fn a_range_of_mips_names_each_subresource_of_the_layer() {
    let chain = TargetDeclaration::new("chain", BC4, TargetExtent::Fixed([8, 8]))
        .mips(4)
        .layers(2);
    assert_eq!(
        chain.mip_range(1, 1..3).collect::<Vec<_>>(),
        [chain.mip(1, 1), chain.mip(1, 2)]
    );
    assert_eq!(chain.mip_range(0, 2..2).count(), 0);
}
