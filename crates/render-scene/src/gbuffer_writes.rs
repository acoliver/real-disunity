//! The producer side of the deferred G-buffer: sources whose fragment stage stores the surface
//! material in the four G-buffer targets instead of lighting it.

const ENCODER: &str = include_str!("gbuffer_writes/encoder_stub.wgsl");
const CAMERA: &str = include_str!("ssao/camera.wgsl");
const TERRAIN_ENTRY: &str = include_str!("gbuffer_writes/terrain.wgsl");

/// The forward terrain entry, which the G-buffer terrain source replaces.
const FORWARD_TERRAIN: &str =
    "@fragment\nfn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32> {";

/// The terrain base source whose `fs_terrain` writes the G-buffer.
///
/// The forward entry is removed rather than renamed, so the module holds no lighting work, and
/// the G-buffer entry keeps the name the terrain program selects. The result is a base source:
/// the terrain program composes its page and layered variants over it as over the stock source.
pub(crate) fn terrain_source(stock: &str) -> String {
    assert_eq!(
        stock.matches(FORWARD_TERRAIN).count(),
        1,
        "the stock terrain source has one forward entry"
    );
    let start = stock.find(FORWARD_TERRAIN).expect("forward terrain entry");
    let length = stock[start..]
        .find("\n}\n")
        .expect("end of the forward terrain entry");
    let end = start + length + "\n}\n".len();
    format!(
        "{}{}\n{CAMERA}\n{ENCODER}\n{TERRAIN_ENTRY}",
        &stock[..start],
        &stock[end..]
    )
}

#[cfg(test)]
mod tests;
