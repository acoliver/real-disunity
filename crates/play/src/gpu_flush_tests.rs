//! What a GPU flush reads from the managed engine without an earlier frame having asked.
//!
//! The portable compute fixture stores one texture (2 pixels wide, sampled with anisotropy 4)
//! and one mesh (4 vertices, 10 indices) and triggers a flush when the slice runs. The handler
//! below never sees a frame, so whatever it reads comes from the flush itself.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use worldrunner_disunity_runtime::GpuFlush;
use worldrunner_world_simulation::graphics::OffscreenWork;

use crate::offscreen::Assets;
use crate::retained_driver::ui_tests::{booted_fixture_host, fixture_turn};

const TEXTURES: u64 = 1 << 40;
const MESHES: u64 = 1 << 41;

/// What one flush read: the texture's width and sampler anisotropy, the mesh's vertex and index
/// counts, and whether a read at the generation just seen came back empty.
type Seen = (u32, u32, u32, usize, bool);

fn read(flush: &mut GpuFlush<'_>) -> Result<Seen, String> {
    let assets: &dyn Assets = &*flush;
    let (texture_id, texture) = (TEXTURES..TEXTURES + 64)
        .find_map(|id| {
            assets
                .texture(id, 0)
                .map_err(|error| error.to_string())
                .transpose()
                .map(|read| read.map(|texture| (id, texture)))
        })
        .transpose()?
        .ok_or("no texture in the flush")?;
    let sampler = assets
        .sampler(texture_id)
        .map_err(|error| error.to_string())?
        .ok_or("no sampler in the flush")?;
    let (mesh_id, mesh) = (MESHES..MESHES + 64)
        .find_map(|id| {
            assets
                .mesh(id, 0)
                .map_err(|error| error.to_string())
                .transpose()
                .map(|read| read.map(|mesh| (id, mesh)))
        })
        .transpose()?
        .ok_or("no mesh in the flush")?;
    let unchanged = assets
        .mesh(mesh_id, mesh.generation)
        .map_err(|error| error.to_string())?
        .is_none()
        && assets
            .texture(texture_id, texture.generation)
            .map_err(|error| error.to_string())?
            .is_none();
    state_thread_group_sizes(flush)?;
    Ok((
        texture.width,
        sampler.aniso_level,
        mesh.vertex_count,
        mesh.indices.len(),
        unchanged,
    ))
}

/// The fixture asks the thread group size of a kernel the data states none for, which a flush
/// answers before the slice goes on.
fn state_thread_group_sizes(flush: &mut GpuFlush<'_>) -> Result<(), String> {
    let snapshot = flush.snapshot().map_err(|error| error.to_string())?;
    for item in &snapshot.work {
        let OffscreenWork::Dispatch { shader, kernel, .. } = item else {
            continue;
        };
        let held = snapshot
            .compute_shaders
            .get(shader)
            .ok_or("a dispatch without its compute shader")?;
        if held.kernels[*kernel as usize].thread_group_size.is_none() {
            flush
                .set_kernel_thread_group_size(shader.to_bits(), *kernel, [4, 2, 1])
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[test]
fn a_flush_reads_the_mesh_texture_and_sampler_no_frame_asked_for() {
    let _turn = fixture_turn();
    let mut host = booted_fixture_host();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&seen);
    host.set_gpu_flush(Box::new(move |flush| {
        log.borrow_mut().push(read(flush)?);
        Ok(())
    }))
    .unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    host.run_slice(&root.join("csharp/PortableFixture/compute.fixture"))
        .unwrap();
    assert!(!seen.borrow().is_empty(), "the fixture triggers a flush");
    for flush in seen.borrow().iter() {
        assert_eq!(
            *flush,
            (2, 4, 4, 10, true),
            "texture 2 wide with anisotropy 4, mesh of 4 vertices and 10 indices, and a read at \
             the generation just seen is empty"
        );
    }
}
