# Playing the original games

`worldrunner-play` runs Snake, Pong or Breakout from its original Unity scene in a window. It fetches the game with `worldrunner-original-games`, plans the scene with `unity-import`, compiles the game's scripts unchanged with `csharp/OriginalGames`, and loads the scene through the `csharp/Disunity.Play` bridge. Each frame advances the world and draws its presentation in the desktop host from `worldrunner-scene-viewer`.

## Command line

```sh
cargo build -p worldrunner-disunity-ffi
cargo run -p worldrunner-play -- pong
cargo run -p worldrunner-play -- breakout --frames 300 --fixed-dt 20 --key 30:a:down --key 90:a:up --capture 120:breakout.png
```

| Option | Meaning |
| --- | --- |
| `--size WxH` | Window size in pixels; 960x540 by default. |
| `--frames N` | Close the window after N frames. Without it the game runs until the window is closed. |
| `--fixed-dt MS` | Advance every frame by MS milliseconds instead of the measured frame time. |
| `--key FRAME:KEY:down\|up` | Press or release a key before frame FRAME. Keys are lowercase names: letters, `space`, `enter`, `escape`, `arrowup`, `arrowdown`, `arrowleft`, `arrowright`. |
| `--capture FRAME:FILE` | Write frame FRAME as it was presented to an RGB PNG. |
| `--seed N` | Seed the game's `Random` before the scene loads, so runs with `--fixed-dt` and the same keys repeat exactly. Without it, as in Unity, each run differs. |
| `--font FILE` | The TrueType file to use for Unity's built-in Arial. On Linux the default is the font `fc-match Arial` selects, usually Liberation Sans, whose metrics match Arial. |

Keyboard input from the window works the same way as scheduled keys. Pong's left paddle uses W and S, and Breakout's paddle uses A and D.

The engine library is found through `DISUNITY_FFI_LIBRARY` or beside the `play` executable, where `cargo build -p worldrunner-disunity-ffi` puts it. Builds go to `target/play/<game>`. The command needs the .NET 8 SDK and system Git, and network access the first time a game is fetched.

Settings the engine does not reproduce and input axes it cannot deliver are printed when the scene loads. Snake is steered with W, A, S and D.

## Built Unity players

`play --player <data directory>` starts a scene of a built Unity player, the `<Game>_Data` directory beside its executable, without a window:

```sh
cargo run -p worldrunner-play -- --player /path/to/Game_Data --scene level0 --frames 60
```

It writes the script layouts of the player's game assemblies with `csharp/Disunity.Layouts`, plans the scene file with `unity-import`'s `plan_player_scene`, and boots the same `Disunity.Play` bridge. The bridge resolves the player's assemblies in its load context, runs the player's startup hooks in Unity's phase order around the scene, loads the scene's plan and the host advances `--frames` frames (one by default) of `--fixed-dt` milliseconds (1/60 s by default). `--scene` names the scene file, `level0` by default. Each step prints a `[player]` line, along with what the plan or the loader leaves out, so the last line before an error is the step that failed, and the error names the hook, component or frame. Builds go to `target/play/player`.

## Tests

`tests/play_window.rs` plays all three games in a 480x270 window and checks the captured pixels. It needs an X display; run it as the lavapipe CI job does:

```sh
env -u WAYLAND_DISPLAY xvfb-run --auto-servernum cargo test -p worldrunner-play --test play_window
```

The test starts each game in a child process without `WAYLAND_DISPLAY` itself. Do the same for the `play` command under `xvfb-run`: with a Wayland session in the environment the window opens there instead, and a window the compositor never shows is never presented.
