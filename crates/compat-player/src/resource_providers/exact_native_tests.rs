use super::*;
use std::path::PathBuf;
use worldrunner::scene::post::ExactSrgb8;

fn root() -> PathBuf {
    PathBuf::from(std::env::var_os("EXACT_RESOURCE_ROOT").expect("explicit exact resource root"))
}

fn host(root: &Path) -> CompatibilityHost {
    let binding = Binding::load(root).expect("current exact resource generation");
    CompatibilityHost::boot(&binding.config()).expect("actual matched native and CLR services")
}
fn negative_controls(
    context: &GpuContext,
    assets: &AssetTable,
    host: &CompatibilityHost,
    world: u64,
    fixture: &native::Fixture,
) -> Result<Value, String> {
    let mut renderer = gpu::Renderer::new(context, assets, gpu::Control::Disconnected)?;
    let disconnected = contracts(host, world, fixture, assets, &mut renderer)
        .err()
        .ok_or("disconnected geometry unexpectedly qualified")?;
    if !disconnected.contains("authored square geometry") {
        return Err(disconnected);
    }
    let wrong_stage = gpu::Renderer::new(context, assets, gpu::Control::WrongStage)
        .err()
        .ok_or("wrong output stage unexpectedly qualified")?;
    if !wrong_stage.contains("output stage") {
        return Err(wrong_stage);
    }
    Ok(json!({"disconnected":disconnected,"wrong_stage":wrong_stage}))
}

fn mapped_images(root: &Path) -> Vec<Value> {
    use std::os::unix::fs::MetadataExt;
    let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
    let config = Binding::load(root).unwrap().config();
    [
        config.ffi_library,
        config.assembly.with_file_name("UnityCompat.dll"),
        config.assembly,
    ]
    .map(|path| {
        let inode = std::fs::metadata(&path).unwrap().ino();
        assert!(
            maps.lines().any(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                fields.last().copied() == path.to_str()
                    && fields.get(4).and_then(|value| value.parse::<u64>().ok()) == Some(inode)
            }),
            "selected native/CLR image must actually be mapped: {}",
            path.display()
        );
        json!({"path":path,"inode":inode,"sha256":compat_kit::files::hash(&path).unwrap()})
    })
    .into_iter()
    .collect()
}

#[test]
#[ignore = "requires explicit frozen native/CLR inputs and a contained GPU run"]
fn actual_native_exact_resources_preserve_original_pixel_contract() {
    let root = root();
    let context = pollster::block_on(GpuContext::request_with_features(
        ExactSrgb8::REQUIRED_FEATURES,
    ))
    .expect("exact profile requires real f32 blend support");
    let assets = assets(&root).unwrap();
    let mut renderer = gpu::Renderer::new(&context, &assets, gpu::Control::Connected).unwrap();
    let mut host = host(&root);
    let world = host.create_world().unwrap();
    let assembly = Binding::load(&root).unwrap().fixture_assembly();
    host.load_fixture(&json!({"world":world,"assemblies":[assembly],
        "graph":{"nodes":[],"components":[],"assets":[],"aliases":{}},"handles":{}}))
        .unwrap();
    let images = mapped_images(&root);
    let fixture = native::Fixture::open(&host, world).unwrap();
    let captures = contracts(&host, world, &fixture, &assets, &mut renderer);
    let negatives = negative_controls(&context, &assets, &host, world, &fixture);
    fixture.finish(&host).unwrap();
    host.unload().unwrap();
    let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
    assert_eq!(roots["data"]["scripts"], 0);
    assert_eq!(roots["data"]["coroutines"], 0);
    assert_eq!(roots["data"]["bindings"], 0);
    assert_eq!(roots["data"]["loaded"], false);
    assert_eq!(roots["data"]["cleanup_pending"], false);
    assert!(roots["data"]["world"].is_null(), "managed world detached");
    host.destroy_world(world).unwrap();
    let native_slots = host.store_statistics().unwrap().0;
    assert_eq!(native_slots, 0);
    let captures = captures.expect("all unchanged exact native-to-GPU resource assertions");
    assert_eq!(captures.len(), 8);
    let output = PathBuf::from(std::env::var_os("EXACT_RESOURCE_EVIDENCE").unwrap());
    std::fs::write(
        output,
        serde_json::to_vec(&json!({"captures":captures,"roots":roots,"negatives":negatives.unwrap(),
            "live_native_slots":native_slots,"images":images,"adapter":format!("{:?}",context.adapter().get_info()),
            "scope":"actual native setters/snapshot/AssetTable/canonical SceneGpu and exact output; no trusted provider registration or original game admission"})).unwrap(),
    )
    .unwrap();
}
