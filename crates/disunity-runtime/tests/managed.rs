use std::path::{Path, PathBuf};
use std::process::Command;
use worldrunner_disunity_runtime::{
    DisunityHost, GpuFlush, GpuValue, GpuWork, HostError, ManagedHostConfig,
};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(command: &mut Command) {
    let output = command
        .output()
        .expect("verification tool must be installed");
    assert!(
        output.status.success(),
        "{command:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

#[test]
fn missing_explicit_artifacts_fail_without_claiming_the_runtime() {
    let config = ManagedHostConfig {
        ffi_library: workspace().join("missing-engine-library"),
        runtime_config: workspace().join("missing-runtime-config"),
        assembly: workspace().join("missing-managed-assembly"),
        bridge_type: "PortableFixture.NativeBridge, PortableFixture".into(),
    };
    for _ in 0..2 {
        assert!(matches!(
            DisunityHost::boot(&config),
            Err(HostError::ManagedArtifactsMissing(_))
        ));
    }
}

fn build_fixture(root: &Path) -> ManagedHostConfig {
    run(
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args([
                "build",
                "--locked",
                "--jobs",
                "2",
                "-p",
                "worldrunner-disunity-ffi",
                "-p",
                "worldrunner-silent-wwise",
            ])
            .current_dir(root),
    );
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let target = if target.is_absolute() {
        target
    } else {
        root.join(target)
    };
    let name = format!(
        "{}worldrunner_disunity_ffi{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let dotnet = std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into());
    run(Command::new(&dotnet)
        .args(["build", "--warnaserror"])
        .arg(root.join("csharp/UnityConsumer/UnityConsumer.csproj")));
    run(Command::new(&dotnet)
        .args(["run", "--project"])
        .arg(root.join("csharp/Disunity.Tests/Disunity.Tests.csproj"))
        .arg("-p:TreatWarningsAsErrors=true")
        .arg("--")
        .arg(target.join("debug").join(&name)));
    run(Command::new(&dotnet)
        .args(["build", "--warnaserror"])
        .arg(root.join("csharp/PortableFixture/PortableFixture.csproj")));
    let managed = root.join("csharp/PortableFixture/bin/Debug/net8.0");
    ManagedHostConfig {
        ffi_library: target.join("debug").join(name),
        runtime_config: managed.join("PortableFixture.runtimeconfig.json"),
        assembly: managed.join("PortableFixture.dll"),
        bridge_type: "PortableFixture.NativeBridge, PortableFixture".into(),
    }
}

fn assert_triangle(host: &DisunityHost) {
    let mut states = host.export_objects().unwrap();
    states.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(states.len(), 3);
    for (state, expected) in states.iter().zip([
        (0.0, 0.0, 0.0, 0),
        (2.0, -3.0, 0.25, 1),
        (4.0, -6.0, 0.5, 2),
    ]) {
        assert!((state.x - expected.0).abs() < f32::EPSILON);
        assert!((state.y - 1.25).abs() < f32::EPSILON);
        assert!((state.z - expected.1).abs() < f32::EPSILON);
        assert!((state.rotation_y_radians - expected.2).abs() < f32::EPSILON);
        assert_eq!(state.material_slot, expected.3);
    }
}

fn assert_lifecycle_soak(host: &mut DisunityHost, fixture: &Path) {
    let mut first_slots = None;
    for cycle in 0..100 {
        host.run_slice(fixture).expect("synthetic fixture loads");
        assert_eq!(host.token_live(), 1, "cycle {cycle}");
        assert!(host.engine_token_live());
        assert_eq!(host.store_statistics().0, 1, "cycle {cycle}");
        assert!(matches!(
            host.run_slice(fixture),
            Err(HostError::WorldAlreadyLoaded)
        ));
        assert_triangle(host);
        host.unload_slice().unwrap();
        assert_eq!(host.token_live(), 0, "cycle {cycle}");
        assert!(!host.engine_token_live());
        let (live, slots) = host.store_statistics();
        assert_eq!(live, 0, "cycle {cycle}");
        if let Some(expected) = first_slots {
            assert_eq!(slots, expected, "cycle {cycle}");
        } else {
            first_slots = Some(slots);
        }
    }
    println!(
        "PASS: 100 real C# load/step/export/unload cycles; native worlds and managed tokens drained"
    );
}

fn assert_failed_boots_release_the_claim(root: &Path, config: &ManagedHostConfig) {
    let mut invalid = config.clone();
    invalid.ffi_library = root.join("missing-engine-library");
    for _ in 0..2 {
        assert!(matches!(
            DisunityHost::boot(&invalid),
            Err(HostError::FfiNotFound(_))
        ));
    }
    invalid = config.clone();
    invalid.bridge_type = "PortableFixture.MissingBridge, PortableFixture".into();
    assert!(matches!(
        DisunityHost::boot(&invalid),
        Err(HostError::Hostfxr(_))
    ));
}

#[test]
fn explicit_bridge_exports_transforms_and_drains_for_one_hundred_cycles() {
    let root = workspace();
    let config = build_fixture(&root);
    assert_failed_boots_release_the_claim(&root, &config);
    let mut host = DisunityHost::boot(&config).expect("explicit host boots after failed boots");
    assert!(matches!(
        DisunityHost::boot(&config),
        Err(HostError::AlreadyBooted)
    ));
    assert!(matches!(host.export_objects(), Err(HostError::NoWorld)));
    let fixture = root.join("csharp/PortableFixture/triangle.fixture");
    assert!(host.run_slice(&root.join("missing.fixture")).is_err());
    assert_eq!(host.token_live(), 0);
    assert!(!host.engine_token_live());
    assert_eq!(host.store_statistics().0, 0);
    assert_failed_runs_drain(&mut host, &root);
    assert_lifecycle_soak(&mut host, &fixture);
    assert_managed_lighting(&mut host, &root);
    assert_presentation_cameras(&mut host, &root);
    assert_player_textures(&mut host, &root);
    assert_player_meshes(&mut host, &root);
    assert_player_distance_field_atlas(&mut host, &root);
    assert_gpu_flush(&mut host, &root);
    host.run_slice(&fixture).unwrap();
    assert_cross_language_writes(&mut host);
    drop(host);
    let host = DisunityHost::boot(&config).unwrap();
    assert_eq!(host.store_statistics().0, 0, "drop unloads an active world");
    assert_eq!(host.token_live(), 0, "drop frees the managed token");
}

fn assert_failed_runs_drain(host: &mut DisunityHost, root: &Path) {
    for _ in 0..2 {
        assert!(
            host.run_slice(&root.join("csharp/PortableFixture/load-failure.fixture"))
                .is_err()
        );
        assert_eq!(host.store_statistics().0, 0);
        assert_eq!(host.token_live(), 0);
        assert!(!host.engine_token_live());
    }
    assert_callback_failure(host, root, "callback-failure.fixture");
    assert_callback_failure(host, root, "long-failure.fixture");
}

fn assert_callback_failure(host: &mut DisunityHost, root: &Path, fixture: &str) {
    host.run_slice(&root.join("csharp/PortableFixture").join(fixture))
        .unwrap();
    let failed = host.advance(1, 100).unwrap_err();
    assert!(
        matches!(&failed, HostError::Managed(message) if message.contains("managed callback failure λ")),
        "{failed:?}"
    );
    assert!(
        host.export_objects().unwrap().is_empty(),
        "failure teardown retires ABI object bindings"
    );
    let faulted = host.advance(0, 1).unwrap_err();
    assert!(matches!(
        faulted,
        HostError::Native {
            reason: worldrunner_disunity_runtime::DisunityErrorReason::Faulted,
            ..
        }
    ));
    host.unload_slice().unwrap();
    assert_eq!(host.store_statistics().0, 0);
    assert_eq!(host.token_live(), 0);
}

fn assert_cross_language_writes(host: &mut DisunityHost) {
    let object = host.export_objects().unwrap()[0].object;
    host.set_position(object, [17.0, 23.0, 31.0]).unwrap();
    assert_eq!(host.managed_position(object).unwrap(), [17.0, 23.0, 31.0]);
    host.advance(1, 100).unwrap();
    assert_eq!(host.managed_position(object).unwrap(), [17.0, 23.0, 31.0]);
    let native = host
        .export_objects()
        .unwrap()
        .into_iter()
        .find(|row| row.object == object)
        .unwrap();
    assert_eq!([native.x, native.y, native.z], [17.0, 23.0, 31.0]);
}

fn assert_managed_lighting(host: &mut DisunityHost, root: &Path) {
    use worldrunner_world_simulation::presentation::SpriteLighting;
    host.run_slice(&root.join("csharp/PortableFixture/lit-sprites.fixture"))
        .unwrap();
    for expected in [[0.05, 0.2, 0.450_000_02, 0.5], [0.1, 0.1, 0.6, 0.5]] {
        let sprites = host.export_sprites().unwrap();
        assert_eq!(sprites.len(), 3);
        for (sprite, _) in sprites {
            assert_eq!(sprite.lighting, SpriteLighting::Global);
            assert_eq!(sprite.color.map(f32::to_bits), expected.map(f32::to_bits));
        }
        let frame = host.export_presentation([80, 40], 8).unwrap();
        assert_eq!(frame.quads.len(), 3);
        for quad in frame.quads {
            assert_eq!(quad.lighting, SpriteLighting::Global);
            assert_eq!(quad.color.map(f32::to_bits), expected.map(f32::to_bits));
        }
        host.advance(1, 100).unwrap();
    }
    host.unload_slice().unwrap();
}

fn assert_presentation_cameras(host: &mut DisunityHost, root: &Path) {
    use worldrunner_camera::CameraLens;
    let fixtures = root.join("csharp/PortableFixture");
    host.run_slice(&fixtures.join("perspective.fixture"))
        .unwrap();
    let frame = host.export_presentation([80, 40], 8).unwrap();
    let lens = frame.camera_state.expect("a presented camera").lens;
    let CameraLens::Perspective {
        vertical_fov_radians,
        near_meters,
        far_meters,
    } = lens
    else {
        panic!("expected a perspective lens, got {lens:?}");
    };
    assert!((vertical_fov_radians - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    assert_eq!((near_meters, far_meters), (0.5, 200.));
    assert!(frame.camera.is_some(), "the lens resolves to a view");
    host.unload_slice().unwrap();
    host.run_slice(&fixtures.join("camera-rect.fixture"))
        .unwrap();
    let failed = host.export_presentation([80, 40], 8).unwrap_err();
    assert!(
        matches!(&failed, HostError::Native { message, .. } if message.contains("rect")),
        "the player log must say why: {failed:?}"
    );
    host.unload_slice().unwrap();
}

fn assert_player_textures(host: &mut DisunityHost, root: &Path) {
    use worldrunner_disunity_runtime::PlayerTexture;
    const FIRST: u64 = 1 << 40;
    host.run_slice(&root.join("csharp/PortableFixture/texture.fixture"))
        .unwrap();
    let (identity, texture) = (FIRST..FIRST + 64)
        .find_map(|id| Some((id, host.player_texture(id, 0).unwrap()?)))
        .expect("the fixture's texture reaches the host");
    assert_eq!(
        texture,
        PlayerTexture {
            format: 4,
            width: 2,
            height: 1,
            mips: 1,
            linear: true,
            generation: 1,
            distance_spread: 0,
            bytes: vec![9, 8, 7, 6, 5, 4, 3, 2],
        }
    );
    assert_player_sampler(host, identity);
    assert_eq!(host.player_texture(identity, 1).unwrap(), None);
    assert_eq!(host.player_texture(identity, 0).unwrap().unwrap(), texture);
    assert_eq!(host.player_texture(FIRST + 1000, 0).unwrap(), None);
    host.unload_slice().unwrap();
}

fn assert_player_sampler(host: &DisunityHost, identity: u64) {
    use worldrunner_disunity_runtime::TextureSampler;
    let sampler = host.texture_sampler(identity).unwrap().expect("sampler");
    assert_eq!(
        sampler,
        TextureSampler {
            generation: 2,
            wrap_u: 1,
            wrap_v: 0,
            wrap_w: 0,
            filter: 2,
            aniso_level: 4,
            mip_bias: 0.0,
        }
    );
    assert_eq!(host.texture_sampler(identity + 1000).unwrap(), None);
}

fn assert_player_meshes(host: &mut DisunityHost, root: &Path) {
    const FIRST: u64 = 1 << 41;
    host.run_slice(&root.join("csharp/PortableFixture/mesh.fixture"))
        .unwrap();
    host.export_presentation([80, 40], 8).unwrap();
    let objects = host.mesh_objects().unwrap();
    assert_eq!(objects.len(), 1, "the fixture shows one mesh renderer");
    let object = &objects[0];
    assert!(object.mesh >= FIRST, "the mesh has a player mesh identity");
    assert_eq!(
        &object.local_to_world[12..15],
        &[3., -2., 0.],
        "the matrix places the object in the view basis"
    );
    assert_eq!(object.slots.len(), 2);
    assert_eq!(object.slots[0].color, [0.25, 0.5, 0.75, 1.]);
    assert_eq!(object.slots[1].color, [1.; 4], "an empty slot draws white");
    assert_eq!(object.slots[0].texture, 0);
    assert_player_mesh(host, object.mesh);
    assert_player_mesh_channels(host, object.mesh);
    assert_eq!(host.player_mesh(object.mesh, 1).unwrap(), None);
    assert_eq!(host.player_mesh(FIRST + 1000, 0).unwrap(), None);
    host.unload_slice().unwrap();
}

fn assert_player_mesh(host: &mut DisunityHost, identity: u64) {
    let mesh = host.player_mesh(identity, 0).unwrap().expect("the mesh");
    assert_eq!(mesh.generation, 1);
    assert_eq!(mesh.positions.len(), 4);
    assert_eq!(mesh.positions[2], [1., 1., 0.]);
    assert!(mesh.normals.is_empty(), "the fixture's mesh has no normals");
    assert_eq!(mesh.uvs[2], [1., 1.]);
    assert!(mesh.colors.is_empty());
    assert_eq!(mesh.indices, vec![0, 2, 1, 0, 3, 2]);
    let runs: Vec<_> = mesh
        .submeshes
        .iter()
        .map(|r| (r.slot, r.first_index, r.index_count))
        .collect();
    assert_eq!(runs, vec![(0, 0, 3), (1, 3, 3)]);
    assert_eq!(
        mesh.left_out, 1,
        "the line sub-mesh is left out of the fixed mesh"
    );
}

fn assert_player_mesh_channels(host: &DisunityHost, identity: u64) {
    use worldrunner_disunity_runtime::MeshRange;
    let mesh = host.mesh_channels(identity).unwrap().expect("the channels");
    assert_eq!((mesh.generation, mesh.vertex_count), (1, 4));
    let shape: Vec<_> = mesh
        .channels
        .iter()
        .map(|c| (c.number, c.dimension, c.data.len()))
        .collect();
    assert_eq!(shape, vec![(0, 3, 12), (2, 4, 16), (4, 2, 8), (5, 2, 8)]);
    assert_eq!(&mesh.channels[0].data[6..9], &[1., 1., 0.]);
    assert_eq!(&mesh.channels[3].data[2..4], &[0.5, 0.]);
    assert_eq!(mesh.indices, vec![0, 2, 1, 0, 3, 2, 0, 1, 1, 2]);
    let range = |start, count, topology| MeshRange {
        start,
        count,
        base_vertex: 0,
        topology,
    };
    assert_eq!(
        mesh.ranges,
        vec![range(0, 3, 0), range(3, 3, 0), range(6, 4, 2)]
    );
    assert_eq!(host.mesh_channels_since(identity, 1).unwrap(), None);
    assert_eq!(host.mesh_channels_since(identity, 0).unwrap(), Some(mesh));
    assert_eq!(host.mesh_channels(identity + 1000).unwrap(), None);
}

fn assert_player_distance_field_atlas(host: &mut DisunityHost, root: &Path) {
    const FIRST: u64 = 1 << 40;
    host.run_slice(&root.join("csharp/PortableFixture/atlas.fixture"))
        .unwrap();
    let atlas = (FIRST..FIRST + 64)
        .find_map(|id| host.player_texture(id, 0).unwrap())
        .expect("the fixture's atlas reaches the host");
    assert_eq!((atlas.format, atlas.width, atlas.height), (1, 64, 64));
    assert_eq!(
        atlas.distance_spread, 4,
        "the distance field glyph marks the atlas"
    );
    assert!(
        atlas.bytes.iter().any(|byte| *byte > 200),
        "the glyph was drawn"
    );
    host.unload_slice().unwrap();
}

/// What the fake executor saw in one flush: the dispatches it ran, as kernel, groups and the names
/// of the values bound.
type Seen = Vec<(u32, [u32; 3], Vec<String>)>;

/// A fake executor: for each dispatch it states a group size for a kernel that has none, and
/// writes `kernel * 100 + groups` into the first buffer bound at the kernel's own slot.
fn fake_executor(
    log: std::rc::Rc<std::cell::RefCell<Vec<Seen>>>,
) -> impl FnMut(&mut GpuFlush<'_>) -> Result<(), String> + 'static {
    move |flush| {
        let work = flush.take_offscreen().map_err(|error| error.to_string())?;
        let mut seen = Seen::new();
        for item in work {
            let GpuWork::Dispatch(dispatch) = item else {
                continue;
            };
            let shader = flush
                .compute_shader(dispatch.shader)
                .map_err(|error| error.to_string())?;
            let kernel = &shader.kernels[dispatch.kernel as usize];
            if kernel.thread_group_size.is_none() {
                flush
                    .set_kernel_thread_group_size(dispatch.shader, dispatch.kernel, [4, 2, 1])
                    .map_err(|error| error.to_string())?;
            }
            let names = dispatch.bindings.iter().map(|b| b.name.clone()).collect();
            seen.push((dispatch.kernel, dispatch.groups, names));
            let Some(GpuValue::Buffer(buffer)) = dispatch
                .bindings
                .iter()
                .find_map(|b| matches!(b.value, GpuValue::Buffer(_)).then(|| b.value.clone()))
            else {
                return Err("a dispatch without a buffer".into());
            };
            let [x, y, z] = dispatch.groups;
            let value = dispatch.kernel * 100 + x * y * z;
            flush
                .write_compute_buffer(buffer, u64::from(dispatch.kernel) * 4, &value.to_le_bytes())
                .map_err(|error| error.to_string())?;
        }
        log.borrow_mut().push(seen);
        Ok(())
    }
}

fn assert_gpu_flush(host: &mut DisunityHost, root: &Path) {
    let fixture = root.join("csharp/PortableFixture/compute.fixture");
    let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    host.set_gpu_flush(Box::new(fake_executor(log.clone())))
        .unwrap();
    host.run_slice(&fixture).unwrap();
    let positions: Vec<_> = host
        .export_objects()
        .unwrap()
        .iter()
        .map(|state| host.managed_position(state.object).unwrap())
        .collect();
    assert!(
        positions.contains(&[6., 105., 421.]),
        "the managed GetData saw what the executor wrote (kernel 0: 6, kernel 1: 105) and the size it stated (4 2 1): {positions:?}"
    );
    let flushes = log.borrow().clone();
    assert_eq!(
        flushes.len(),
        2,
        "one flush for the group size and one for the read"
    );
    let names = vec!["_Width".to_owned(), "_Output".to_owned()];
    assert_eq!(
        flushes[0],
        vec![(0, [3, 2, 1], names.clone()), (1, [5, 1, 1], names)],
        "the first flush ran both dispatches in call order, each with the values it had"
    );
    assert!(
        flushes[1].is_empty(),
        "work already taken is not taken again"
    );
    host.unload_slice().unwrap();
    assert_gpu_flush_failure(host, &fixture);
    host.set_gpu_flush(Box::new(|_| Ok(()))).unwrap();
    let unstated = host.run_slice(&fixture).unwrap_err();
    assert!(
        matches!(&unstated, HostError::SliceRun { message, .. } if message.contains("ComputeShader.GetKernelThreadGroupSizes")),
        "an executor that states no group size leaves the managed call to throw: {unstated:?}"
    );
}

fn assert_gpu_flush_failure(host: &mut DisunityHost, fixture: &Path) {
    host.set_gpu_flush(Box::new(|_| Err("the device is lost".into())))
        .unwrap();
    let failed = host.run_slice(fixture).unwrap_err();
    assert!(
        matches!(&failed, HostError::SliceRun { message, .. } if message.contains("GPU flush") && message.contains("the device is lost")),
        "a failing flush fails the managed call and names the flush: {failed:?}"
    );
    assert_eq!(host.store_statistics().0, 0);
}
