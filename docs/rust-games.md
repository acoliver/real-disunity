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
| `script` | A sandboxed Lua 5.4 host for game logic. |

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

    // Input is timestamped and becomes visible in the next advance.
    let space = KeyCode(32);
    world.push_input(&[InputEvent {
        timestamp: world.clock().unscaled_time,
        action: InputAction::Key(space, true),
    }])?;
    world.advance(frame)?;
    assert!(world.input(InputPhase::Dynamic).down.contains(&space));
    Ok(())
}
```

A few rules that the world enforces:

- Every fallible operation returns a typed error. Invalid input, stale handles and exhausted limits are rejected before anything changes.
- Handles are generation checked. Using a handle after its object is destroyed returns an error, even after the storage slot has been reused.
- `advance` returns `Advance::Callback` when a script needs to run. A Rust game without scripts only ever sees `Advance::Complete`. The C# host is the main user of callbacks.
- Key codes and axis identifiers are numbers the game chooses. `World::set_input_axes` binds keys to axes.

Physics works in the XY plane by default, with gravity of -9.81 m/s² on Y. `World::set_physics_settings` selects the XZ plane, gravity and solver limits. Contacts are available from `World::contacts()` after each step, and `raycast`, `overlap_point` and `overlap_shape` query the same colliders.

## Draw a 2D world in a window

The renderer never reads world state directly. Each frame, `extract_world` turns the world's sprites, tiles and active camera into a render plan, the renderer prepares it, and the viewer presents it. Texture pixels come from the game in a `SpriteTextures` table keyed by the texture number that each `Sprite` refers to.

The viewer drives the game through the `LiveDriver` trait. This is a complete windowed program that draws one orange square:

```rust
use std::collections::BTreeMap;

use worldrunner::glam::Vec3;
use worldrunner::scene::{
    gpu::simulation::WorldSceneContribution,
    plan::{SceneTexture, TexelFormat, TextureColorSpace, simulation::{SpriteTextures, extract_world}},
    viewer::{LiveDriver, LiveFrame, LiveOptions, LiveTick, run_live},
};
use worldrunner::simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, TimeSpan, World, WorldSettings,
    presentation::{Camera, Sprite, SpriteRenderer},
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

impl LiveDriver for Game {
    fn update(&mut self, tick: &LiveTick) -> Result<LiveFrame, String> {
        let nanos = u64::try_from(tick.dt.as_nanos()).map_err(|e| e.to_string())?;
        let dt = TimeSpan::from_nanos(nanos).map_err(|e| e.to_string())?;
        self.world.advance(dt).map_err(|e| e.to_string())?;
        let viewport = [tick.viewport.width, tick.viewport.height];
        let plan = extract_world(&self.world, &self.textures, viewport).map_err(|e| e.to_string())?;
        let contribution = WorldSceneContribution::new(plan).map_err(|e| e.to_string())?;
        Ok(LiveFrame::from_world(contribution))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut game = Game::new()?;
    run_live(&mut game, LiveOptions::new("My game", [800, 600]))?;
    Ok(())
}
```

Keyboard, focus and resize events arrive in `LiveTick::input`. Convert the keys you care about into `InputEvent`s and push them before calling `advance`. Set `LiveFrame::exit` to close the window. For UI text, enable the `ui` feature, register a TrueType font in a `FontBook`, add `Canvas`, `RectTransform` and `Text` components, and append `extract_ui` output to the plan with `append_ui`.

## Draw a 3D scene

3D content goes through `RenderScene`, a validated description of meshes, materials, textures, terrain, lights and effects. A game builds one with the constructors in `worldrunner::scene::plan` and hands it to `worldrunner::scene::viewer::run` for a window, or to `worldrunner::scene::gpu::offscreen::capture_scene` for a headless image. `run_with_updates` gives a per-frame callback that can move, add and hide instances without re-uploading the scene.

The viewer example in `crates/worldrunner/examples/scene-viewer.rs` builds a scene with meshes, terrain, water and shadows from code, and is the best reference for the scene API. [Rendering features](rendering.md) lists what a scene can turn on.

## Next steps

- [Rendering features](rendering.md) describes the 2D and 3D renderer options.
- [Writing C# scripts](csharp-games.md) shows how `MonoBehaviour` code runs on this same world.
- The [developer documentation](../dev-docs/README.md) has the exact contracts for scheduling, physics, UI and rendering, if you need behavior this page does not cover.
