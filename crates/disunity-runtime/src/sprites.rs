//! Typed observations of the same native image used by managed SpriteRenderer.
use crate::{DisunityHost, HostError, native};
use worldrunner_world_simulation::presentation::{Sprite, SpriteRenderer};

impl DisunityHost {
    pub fn export_sprites(&self) -> Result<Vec<(SpriteRenderer, glam::Mat4)>, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        type Export = unsafe extern "C" fn(
            u64,
            *mut native::DisunitySprite,
            *mut native::DisunityMatrix,
            u32,
            *mut u32,
        ) -> native::DisunityResult;
        // The image stays loaded for the host's lifetime; its scalar ABI is generated.
        let export: Export = unsafe {
            *self
                ._library
                .get(b"disunity_world_export_sprites\0")
                .map_err(|_| HostError::MissingSymbol("disunity_world_export_sprites".into()))?
        };
        let mut count = 0;
        let result = unsafe {
            export(
                world,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                &mut count,
            )
        };
        if result.code != 0 {
            return Err(HostError::Abi(result.code));
        }
        let mut sprites = vec![native::DisunitySprite::default(); count as usize];
        let mut matrices = vec![native::DisunityMatrix::default(); count as usize];
        let mut written = 0;
        let result = unsafe {
            export(
                world,
                sprites.as_mut_ptr(),
                matrices.as_mut_ptr(),
                count,
                &mut written,
            )
        };
        if result.code != 0 {
            return Err(HostError::Abi(result.code));
        }
        if written != count {
            return Err(HostError::Managed(
                "native sprite count changed during copy-out".into(),
            ));
        }
        Ok(sprites
            .into_iter()
            .zip(matrices)
            .map(|(v, matrix)| decode(v, matrix))
            .collect())
    }
}

fn decode(
    v: native::DisunitySprite,
    matrix: native::DisunityMatrix,
) -> (SpriteRenderer, glam::Mat4) {
    let sprite = (v.present == 1).then_some(Sprite {
        identity: v.identity,
        texture: v.texture,
        rect: [v.x, v.y, v.w, v.h],
        pivot: [v.pivot_x, v.pivot_y],
        pixels_per_unit: v.pixels_per_unit,
        texture_size: [v.width, v.height],
    });
    let renderer = SpriteRenderer {
        sprite,
        enabled: v.enabled == 1,
        color: [v.r, v.g, v.b, v.a],
        flip_x: v.flip_x == 1,
        flip_y: v.flip_y == 1,
        sorting_layer: v.sorting_layer,
        sorting_order: v.sorting_order,
        lighting: worldrunner_world_simulation::presentation::SpriteLighting::try_from(v.lighting)
            .expect("native sprite material is validated"),
    };
    let matrix = glam::Mat4::from_cols_array(&[
        matrix.m0, matrix.m1, matrix.m2, matrix.m3, matrix.m4, matrix.m5, matrix.m6, matrix.m7,
        matrix.m8, matrix.m9, matrix.m10, matrix.m11, matrix.m12, matrix.m13, matrix.m14,
        matrix.m15,
    ]);
    (renderer, matrix)
}
