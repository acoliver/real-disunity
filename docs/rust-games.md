# Writing a Rust game

A native Rust game uses the engine directly: it owns a `World`, adds objects and components to it, advances it each frame, and passes what the world contains to the renderer. This is the same world that runs C# scripts, so everything here also explains what a `MonoBehaviour` is doing underneath.

The examples on this page compile against the current `main` branch.

## Add the dependency

The engine is not published to crates.io. Depend on the `worldrunner` facade crate from Git and pin a commit, because the API changes between commits:

```toml
[dependencies]
worldrunner = { git = "https://github.com/acoliver/real-disunity", rev = "<commit>", features = ["scene-viewer"] }
```

`worldrunner` re-exports the engine crates behind feature flags, so a game needs no other engine dependency. It also re-exports `glam` for vectors and quaternions.

| Feature | What it adds |
| --- | --- |
| `simulation` | `worldrunner::simulation`: the native world, physics and input. CPU only. |
| `ui` | Canvas, RectTransform and text layout in `worldrunner::ui`. CPU only. |
| `scene` | `worldrunner::scene`: render planning, the GPU renderer, post-processing and headless capture. |
| `scene-viewer` | `worldrunner::scene::viewer`: a desktop window that presents a scene. Implies `scene`. |
| `world` (default) | World documents, deterministic world generation and save files. |
| `assets` (default) | Asset identities, packages, meshes, materials and textures. |
| `script` | A sandboxed Lua 5.4 host for game logic and a game's original scripts; see [Lua scripts](lua-scripts.md). |

## Build and advance a world

A `World` holds objects. Each object has a transform and any number of typed components: 2D bodies and colliders, sprites, cameras, UI elements and script slots. The world advances on an exact rational clock. Each call to `advance` runs whole fixed steps (1/50 second by default) with physics inside them, then the per-frame update.

This program drops a ball onto a floor and then reads a key press:

```rust
use worldrunner::glam::{Vec2, Vec3};
use worldrunner::simulation::{
    Advance, ComponentDesc, InputAction, InputEvent, InputPhase, KeyCode, LocalTransform,
    ObjectDesc, TimeSpan, World, WorldSettings,
    physics2d::{Body2d, Collider2d, Shape2d},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut world = World::new(WorldSettings::default())?;

    // A static floor: a collider with no body.
    let floor = world.create_object(ObjectDesc::default())?;
    world.add_component(
        floor,
        ComponentDesc::Collider2d(Collider2d {
            shapes: vec![Shape2d::Box { half_extents: Vec2::new(10.0, 0.5) }],
            ..Default::default()
        }),
    )?;

    // A dynamic ball five metres above it.
    let ball = world.create_object(ObjectDesc {
        local_transform: LocalTransform {
            translation: Vec3::new(0.0, 5.0, 0.0),
            ..Default::default()
        },
        ..Default::default()
    })?;
    world.add_component(
        ball,
        ComponentDesc::Body2d(Body2d { restitution: 0.5, ..Default::default() }),
    )?;
    world.add_component(
        ball,
        ComponentDesc::Collider2d(Collider2d {
            shapes: vec![Shape2d::Circle { radius: 0.5 }],
            ..Default::default()
        }),
    )?;

    // Two seconds at 60 frames per second.
    let frame = TimeSpan::new(1, 60)?;
    for _ in 0..120 {
        match world.advance(frame)? {
            Advance::Complete(_) => {}
            other => return Err(format!("no scripts are attached: {other:?}").into()),
        }
    }
    println!("ball height: {:.2}", world.world_matrix(ball)?.w_axis.y);

    // Input carries an unscaled timestamp later than the time the world has
    // already reached (two seconds here), and is seen by the next advance.
    let space = KeyCode(32);
    world.push_input(&[InputEvent {
        timestamp: TimeSpan::new(121, 60)?,
        action: InputAction::Key(space, true),
    }])?;
    world.advance(frame)?;
    assert!(world.input(InputPhase::Dynamic).down.contains(&space));
    Ok(())
}
```

A few rules that the world enforces:

- Every fallible operation returns a typed error. Invalid input, stale handles and exhausted limits are rejected before anything changes.
- Handles are generation checked. `destroy` takes effect at the end of the frame, as in Unity; after that, using the handle returns an error, even once the storage slot has been reused.
- `advance` returns `Advance::Callback` when a script needs to run. A Rust game without scripts only ever sees `Advance::Complete`. The C# host is the main user of callbacks.
- Input timestamps must not go backwards, and an event at or before the time of the last completed frame is rejected with `InputOrder`.
- Key codes and axis identifiers are numbers the game chooses. `World::register_input_axis` defines a named axis from negative and positive keys, with Unity-style gravity, sensitivity, dead zone and snap, and `World::input_axis(phase, name, raw)` reads it.

Physics works in the XY plane by default, with gravity of -9.81 m/s² on Y. `World::set_physics_settings` selects the XZ plane, gravity and solver limits. Contacts are available from `World::contacts()` after each step, and `raycast`, `overlap_point` and `overlap_shape` query the same colliders.

## Draw a 2D world in a window

The renderer never reads world state directly. Each frame, `extract_world` turns the world's sprites, tiles and active camera into a render plan, the renderer prepares it, and the viewer presents it. Texture pixels come from the game in a `SpriteTextures` table keyed by the texture number that each `Sprite` refers to; the game passes that table to `WorldSceneContribution::new` beside the plan.

The desktop viewer is the retained host. A game implements `RetainedDriver` and `RetainedHost::run` calls it once per frame. Each frame the game advances its world, extracts the render plan and applies it to the host as `RetainedUpdate::World`. The frame carries the camera's view and background colour, so a world host needs no scene of its own: it starts from an empty `RenderScene` and ignores the options' `pose`, `camera` and `orbit`, which `ViewerOptions` still requires. This is a complete windowed program that draws one orange square:

```rust
use std::{collections::BTreeMap, sync::Arc};

use worldrunner::glam::Vec3;
use worldrunner::scene::{
    camera::ViewConvention,
    gpu::{CameraPose, simulation::WorldSceneContribution},
    plan::{
        RenderScene, SceneCamera, SceneGeometry, SceneParts, SceneResources, SceneTexture,
        TexelFormat, TextureColorSpace, simulation::SpriteTextures,
    },
    viewer::{
        ActivityReason, CapturePoolPolicy, FrameProfile, GameRendering, OrbitLimits, PresentMode,
        PresentationBudget, PresentationCadence, RenderClock, RetainedDriver, RetainedHost,
        RetainedTick, RetainedUpdate, ToneMap, ViewerOptions, ViewerStart, WindowMode,
        WindowSizeUnit,
    },
};
use worldrunner::simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, TimeSpan, World, WorldSettings,
    presentation::{Camera, Sprite, SpriteRenderer, extraction::extract_world},
};

struct Game {
    world: World,
    textures: SpriteTextures,
}

impl Game {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut world = World::new(WorldSettings::default())?;
        let camera = world.create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(0.0, 0.0, -10.0),
                ..Default::default()
            },
            ..Default::default()
        })?;
        world.add_component(camera, ComponentDesc::Camera(Camera::default()))?;

        let square = world.create_object(ObjectDesc::default())?;
        world.add_component(
            square,
            ComponentDesc::Sprite(SpriteRenderer {
                sprite: Some(Sprite {
                    identity: 0,
                    texture: 1,
                    rect: [0.0, 0.0, 8.0, 8.0],
                    pivot: [0.5, 0.5],
                    pixels_per_unit: 8.0,
                    texture_size: [8, 8],
                }),
                color: [1.0, 0.5, 0.0, 1.0],
                ..Default::default()
            }),
        )?;

        let white = SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Srgb,
            width: 8,
            height: 8,
            mips: 1,
            bytes: vec![255; 8 * 8 * 4].into_boxed_slice(),
        };
        let textures = SpriteTextures { textures: BTreeMap::from([(1, white)]) };
        Ok(Self { world, textures })
    }
}

impl RetainedDriver for Game {
    fn update(&mut self, viewer: &mut RetainedHost, tick: &RetainedTick) -> Result<(), String> {
        let nanos = u64::try_from(tick.dt.as_nanos()).map_err(|e| e.to_string())?;
        let dt = TimeSpan::from_nanos(nanos).map_err(|e| e.to_string())?;
        self.world.advance(dt).map_err(|e| e.to_string())?;
        let plan = extract_world(&self.world, viewer.size()).map_err(|e| e.to_string())?;
        let frame = WorldSceneContribution::new(plan, &self.textures.textures)
            .map_err(|e| e.to_string())?;
        viewer
            .apply(RetainedUpdate::World(Arc::new(frame)))
            .map_err(|e| e.to_string())
    }
}

fn options() -> ViewerOptions {
    ViewerOptions {
        activity_reason: ActivityReason::default(),
        presentation_budget: PresentationBudget::default().into(),
        capture_pool: CapturePoolPolicy::default(),
        camera_convention: ViewConvention::RIGHT_HANDED,
        ssr: None,
        title: "My game".into(),
        window_size_unit: WindowSizeUnit::default(),
        window_mode: WindowMode::default(),
        size: [800, 600],
        pose: CameraPose {
            eye_millimeters: [0, -2000, 2000],
            target_millimeters: [0, 0, 0],
        },
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 1_000_000,
        },
        orbit: OrbitLimits {
            min_distance_millimeters: 100.0,
            max_distance_millimeters: 10_000.0,
            min_elevation_radians: -1.4,
            max_elevation_radians: 1.4,
        },
        shadows: None,
        frame_profile: FrameProfile::Hdr {
            samples: 1,
            tone_map: ToneMap::Clamp,
        },
        frames_in_flight: 2,
        gpu_timing: false,
        present_mode: PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        texture_tiers: None,
        adapter_callback: None,
        cadence: PresentationCadence::default(),
        render_clock: RenderClock::default(),
        game: GameRendering::default(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut game = Game::new()?;
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![])?,
        geometry: SceneGeometry::new(vec![], vec![])?,
        terrain: None,
        effects: None,
    })?;
    RetainedHost::new(scene, options())?.run(ViewerStart::new()?, &mut game)?;
    Ok(())
}
```

Keyboard, focus and resize events arrive in `RetainedDriver::event` as `RetainedEvent::Input`. Convert the keys you care about into `InputEvent`s and push them before calling `advance`. Call `RetainedEventHost::request_exit` to close the window. The tick's `dt` is the time the window took for the last frame; a replay that needs a fixed step uses its own constant in place of it, and `RetainedHost::request_capture` returns the presented frame as pixels in a `RetainedEvent::Capture`. For UI text, enable the `ui` feature, register a TrueType font in a `FontBook`, add `Canvas`, `RectTransform` and `Text` components, and append `extract_ui` output to the plan with `presentation::extraction::append_ui`.

For a caller-owned immediate overlay without Canvas objects, see
[Drawing an immediate overlay](immediate-ui.md). It builds a detached presentation
for the existing retained compositor.

## Draw a 3D scene

3D content goes through `RenderScene`, a validated description of meshes, materials, textures, terrain, lights and effects. A game builds one with the constructors in `worldrunner::scene::plan` and hands it to `RetainedHost::new` for a window, or to `worldrunner::scene::gpu::offscreen::capture_scene` for a headless image. The same `RetainedDriver` that draws a 2D world edits a 3D scene: `RetainedUpdate::Add`, `Replace`, `Remove` and `Objects` move, add and hide instances without re-uploading the scene, and `RetainedUpdate::Camera` moves the camera. The host leaves camera control to the game; `OrbitNavigation` turns the window's pointer, wheel and the R and Home keys into orbit poses.

The viewer example in `crates/worldrunner/examples/scene-viewer.rs` builds a scene with meshes, terrain, water and shadows from code, and orbits it with `OrbitNavigation`. It is the best reference for the scene API. [Rendering features](rendering.md) lists what a scene can turn on.

## Next steps

- [Rendering features](rendering.md) describes the 2D and 3D renderer options.
- [Writing C# scripts](csharp-games.md) shows how `MonoBehaviour` code runs on this same world.
- The [developer documentation](../dev-docs/README.md) has the exact contracts for scheduling, physics, UI and rendering, if you need behavior this page does not cover.
