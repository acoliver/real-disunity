//! Complete prepared observations from the same loaded engine image as C#.
use crate::{DisunityHost, HostError, native};
use worldrunner_camera::{
    CameraLens, CameraPose, CameraState, ViewConvention, ViewHandedness, Viewport, resolve,
};
use worldrunner_world_simulation::presentation::{
    CoverageTexture as SceneTexture,
    extraction::{SpriteQuad, WorldRenderPlan},
};

impl DisunityHost {
    /// Extracts sprites, tiles, camera and native-layout glyph coverage in one frame.
    pub fn export_presentation(
        &mut self,
        viewport: [u32; 2],
        next_texture: u64,
    ) -> Result<WorldRenderPlan, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        type Prepare = unsafe extern "C" fn(
            u64,
            u32,
            u32,
            u64,
            *mut native::DisunityPresentation,
        ) -> native::DisunityResult;
        type Quad =
            unsafe extern "C" fn(u64, u32, *mut native::DisunityQuad) -> native::DisunityResult;
        // All symbols and generated records belong to the image retained by this host.
        let prepare: Prepare = unsafe {
            *self
                ._library
                .get(b"disunity_presentation_prepare\0")
                .map_err(|_| HostError::MissingSymbol("disunity_presentation_prepare".into()))?
        };
        let quad: Quad = unsafe {
            *self
                ._library
                .get(b"disunity_presentation_quad\0")
                .map_err(|_| HostError::MissingSymbol("disunity_presentation_quad".into()))?
        };
        let coverage: Coverage = unsafe {
            *self
                ._library
                .get(b"disunity_presentation_coverage\0")
                .map_err(|_| HostError::MissingSymbol("disunity_presentation_coverage".into()))?
        };
        let mut header = native::DisunityPresentation::default();
        check(unsafe { prepare(world, viewport[0], viewport[1], next_texture, &mut header) })?;
        let state = (header.present == 1).then(|| camera_state(header));
        let camera = state
            .as_ref()
            .map(resolve)
            .transpose()
            .map_err(|e| HostError::Managed(e.to_string()))?;
        let mut plan = WorldRenderPlan {
            camera,
            camera_state: state,
            clear: [header.r, header.g, header.b, header.a],
            quads: vec![],
            textures: Default::default(),
        };
        for index in 0..header.quads {
            let mut q = native::DisunityQuad::default();
            check(unsafe { quad(world, index, &mut q) })?;
            plan.quads.push(decode_quad(q));
        }
        plan.textures = read_coverage(world, header.textures, coverage)?;
        Ok(plan)
    }
}
fn check(result: native::DisunityResult) -> Result<(), HostError> {
    if result.code == 0 {
        Ok(())
    } else {
        Err(HostError::Abi(result.code))
    }
}
fn camera_state(h: native::DisunityPresentation) -> CameraState {
    CameraState {
        pose: CameraPose {
            eye_meters: glam::DVec3::new(h.eye_x, h.eye_y, h.eye_z),
            forward: glam::DVec3::new(h.forward_x, h.forward_y, h.forward_z),
            up: glam::DVec3::new(h.up_x, h.up_y, h.up_z),
        },
        lens: CameraLens::Orthographic {
            half_height_meters: h.half_height,
            near_meters: h.near,
            far_meters: h.far,
        },
        viewport: Viewport {
            width: h.width,
            height: h.height,
        },
        convention: ViewConvention {
            handedness: ViewHandedness::DirectXLeftHanded,
        },
    }
}
fn decode_quad(q: native::DisunityQuad) -> SpriteQuad {
    SpriteQuad {
        overlay: q.overlay == 1,
        corners: [
            [q.p0_x, q.p0_y, q.p0_z],
            [q.p1_x, q.p1_y, q.p1_z],
            [q.p2_x, q.p2_y, q.p2_z],
            [q.p3_x, q.p3_y, q.p3_z],
        ],
        uv_rect: [q.u0, q.v0, q.u1, q.v1],
        color: [q.r, q.g, q.b, q.a],
        texture: (q.textured == 1).then_some(q.texture),
        sorting_layer: q.sorting_layer,
        sorting_order: q.sorting_order,
    }
}

type Coverage = unsafe extern "C" fn(
    u64,
    u32,
    *mut native::DisunityCoverage,
    *mut u8,
    u32,
) -> native::DisunityResult;

fn read_coverage(
    world: u64,
    count: u32,
    coverage: Coverage,
) -> Result<std::collections::BTreeMap<u64, SceneTexture>, HostError> {
    let mut textures = std::collections::BTreeMap::new();
    for index in 0..count {
        let mut meta = native::DisunityCoverage::default();
        check(unsafe { coverage(world, index, &mut meta, std::ptr::null_mut(), 0) })?;
        let mut bytes = vec![0; meta.length as usize];
        check(unsafe { coverage(world, index, &mut meta, bytes.as_mut_ptr(), meta.length) })?;
        textures.insert(
            meta.identity,
            SceneTexture {
                width: meta.width,
                height: meta.height,
                bytes: bytes.into_boxed_slice(),
            },
        );
    }
    Ok(textures)
}
