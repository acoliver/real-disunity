#![cfg(all(feature = "scene", feature = "simulation", feature = "ui"))]
use glam::{Quat, Vec3};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use worldrunner::scene::plan::simulation::{SpriteTextures, extract_world};
use worldrunner::simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
    presentation::{Sprite, SpriteRenderer},
};
use worldrunner_disunity_runtime::{DisunityHost, ManagedHostConfig};

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn rust_and_csharp_worlds_produce_equivalent_sprite_contributions() {
    let (config, target) = setup();
    let fixture = target.join(format!("sprite-equivalence-{}.txt", std::process::id()));
    std::fs::write(&fixture, "portable-sprites-v1").unwrap();
    let mut host = DisunityHost::boot(&config).unwrap();
    host.run_slice(&fixture).unwrap();
    let managed = host.export_presentation([80, 40], 8).unwrap();
    let managed = worldrunner::scene::plan::simulation::from_presentation(managed, &textures());
    let native = native_world();
    assert_eq!(native, managed);
    assert!(managed.camera.is_some());
    assert!(managed.quads.iter().any(|q| q.overlay));
    assert!(managed.quads.len() >= 5);
    let prepared =
        worldrunner::scene::gpu::simulation::WorldSceneContribution::new(managed).unwrap();
    assert!(prepared.quads.len() >= 5);
    #[cfg(feature = "scene-viewer")]
    {
        let live = worldrunner::scene::viewer::LiveFrame::from_world(
            worldrunner::scene::gpu::simulation::WorldSceneContribution::new(native).unwrap(),
        );
        assert_eq!(
            prepared.clear.map(f64::to_bits),
            live.clear.map(f64::to_bits)
        );
        assert_eq!(prepared.quads.len(), live.quads.len());
        for (headless, live) in prepared.quads.iter().zip(live.quads) {
            assert_eq!(headless.corners, live.corners);
            assert_eq!(headless.texture, live.texture);
        }
    }

    host.unload_slice().unwrap();
    assert!(host.export_sprites().is_err());
    std::fs::remove_file(fixture).unwrap();
}

fn setup() -> (ManagedHostConfig, PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    run(
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args([
                "build",
                "--locked",
                "--jobs",
                "2",
                "-p",
                "worldrunner-disunity-ffi",
            ])
            .current_dir(&root),
    );
    run(
        Command::new(std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into()))
            .args(["build", "--warnaserror"])
            .arg(root.join("csharp/PortableFixture/PortableFixture.csproj")),
    );
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let target = if target.is_absolute() {
        target
    } else {
        root.join(target)
    };
    let managed = root.join("csharp/PortableFixture/bin/Debug/net8.0");
    let config = ManagedHostConfig {
        ffi_library: target.join("debug").join(format!(
            "{}worldrunner_disunity_ffi{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        )),
        runtime_config: managed.join("PortableFixture.runtimeconfig.json"),
        assembly: managed.join("PortableFixture.dll"),
        bridge_type: "PortableFixture.NativeBridge, PortableFixture".into(),
    };
    (config, target)
}

fn native_world() -> worldrunner::scene::plan::simulation::WorldRenderPlan {
    let mut world = World::new(WorldSettings::default()).unwrap();
    add_sprites(&mut world);
    add_camera(&mut world);
    let fonts = add_ui(&mut world);
    let mut plan = extract_world(&world, &textures(), [80, 40]).unwrap();
    let ui = worldrunner::scene::plan::ui::extract_ui(&world, &fonts, [80, 40]).unwrap();
    worldrunner::scene::plan::simulation::append_ui(&mut plan, ui).unwrap();
    plan
}
fn add_sprites(world: &mut World) {
    for index in 0..3u16 {
        let i = f32::from(index);
        let object = world
            .create_object(ObjectDesc {
                local_transform: LocalTransform {
                    translation: Vec3::new(i * 2., 1.25, -i * 3.),
                    rotation: Quat::from_rotation_y(i * 0.25),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        world
            .add_component(
                object,
                ComponentDesc::Sprite(SpriteRenderer {
                    sprite: Some(Sprite {
                        identity: 0,
                        texture: 7,
                        rect: [0., 0., 2., 4.],
                        pivot: [0.5, 0.5],
                        pixels_per_unit: 2.,
                        texture_size: [8, 8],
                    }),
                    color: [0.2, 0.4, 0.6, 0.5],
                    flip_x: true,
                    sorting_layer: 1,
                    sorting_order: 2,
                    ..Default::default()
                }),
            )
            .unwrap();
    }
}
fn add_camera(world: &mut World) {
    let camera = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(0., 0., -10.),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    world
        .add_component(
            camera,
            ComponentDesc::Camera(worldrunner::simulation::presentation::Camera::default()),
        )
        .unwrap();
}
fn add_ui(world: &mut World) -> worldrunner::simulation::ui::FontBook {
    use worldrunner::simulation::{
        ParentSpace,
        ui::{Canvas, FontBook, RectTransform, TextRun},
    };
    let canvas = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            canvas,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    world
        .add_component(canvas, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let label = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            label,
            ComponentDesc::RectTransform(RectTransform {
                size_delta: glam::Vec2::new(30., 20.),
                anchored_position: glam::Vec2::new(11., -3.),
                ..Default::default()
            }),
        )
        .unwrap();
    world
        .set_parent(label, Some(canvas), ParentSpace::KeepLocal)
        .unwrap();
    let mut fonts = FontBook::new(worldrunner::simulation::ui::UiLimits::default()).unwrap();
    let font = fonts.register(test_font::font()).unwrap();
    world
        .add_component(
            label,
            ComponentDesc::Text(TextRun {
                text: "AB".into(),
                font_size: 12.,
                ..TextRun::new(font)
            }),
        )
        .unwrap();
    fonts
}

#[path = "../../world-simulation/tests/ui_font/mod.rs"]
mod test_font;
fn textures() -> SpriteTextures {
    use worldrunner::scene::plan::{SceneTexture, TexelFormat, TextureColorSpace};
    SpriteTextures {
        textures: std::collections::BTreeMap::from([(
            7,
            SceneTexture::Texels {
                format: TexelFormat::Rgba8Unorm,
                color_space: TextureColorSpace::Srgb,
                width: 8,
                height: 8,
                mips: 1,
                bytes: vec![255; 8 * 8 * 4].into_boxed_slice(),
            },
        )]),
    }
}
