use worldrunner::glam::Vec3;
use worldrunner::scene::{
    camera::ResolvedView,
    gpu::{dynamic::DynamicQuad, offscreen::RgbaImage},
    plan,
};

use super::capture::pixel;

pub const POINTS: [[f32; 3]; 3] = [[-0.8, 0.0, 1.0], [0.9, 0.0, 0.3], [-0.4, 0.0, -1.0]];
pub const COLORS: [[u8; 4]; 3] = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];

pub fn quads() -> Vec<DynamicQuad> {
    POINTS
        .into_iter()
        .zip(COLORS)
        .map(|(position, color)| DynamicQuad {
            position,
            rect: [-0.16, -0.16, 0.16, 0.16],
            tint: color.map(|v| f32::from(v) / 255.0),
            ..DynamicQuad::default()
        })
        .collect()
}

pub fn assert_landmarks(image: &RgbaImage, view: &ResolvedView) {
    for (point, color) in POINTS.into_iter().zip(COLORS) {
        let point = Vec3::from_array(point);
        let screen = view.world_to_screen(point).unwrap();
        let [x, y] = texel(screen);
        assert_eq!(
            pixel(image, x, y),
            color,
            "landmark {point:?} at {screen:?}"
        );
        assert!(
            view.screen_to_world(screen)
                .unwrap()
                .abs_diff_eq(point, 0.0001)
        );
        let reconstruction = view.reconstruction_projection_matrix()
            * view.reconstruction_view_matrix()
            * point.extend(1.0);
        let raster = view.view_projection_matrix() * point.extend(1.0);
        assert!((reconstruction / reconstruction.w).abs_diff_eq(raster / raster.w, 0.0001));
    }
}

pub fn texel(screen: Vec3) -> [u32; 2] {
    screen.truncate().floor().as_uvec2().to_array()
}

pub fn texture(revision: u8) -> plan::SceneTexture {
    let mut pixels = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
    ];
    match revision {
        0 => {}
        1 => pixels.rotate_left(1),
        2 => pixels.swap(0, 3),
        _ => panic!("unknown authored texture revision"),
    }
    plan::SceneTexture::Texels {
        color_space: plan::TextureColorSpace::Srgb,
        format: plan::TexelFormat::Rgba8Unorm,
        width: 8,
        height: 8,
        mips: 1,
        bytes: (0..8)
            .flat_map(|y| (0..8).flat_map(move |x| pixels[(y / 4) * 2 + x / 4]))
            .collect(),
    }
}
