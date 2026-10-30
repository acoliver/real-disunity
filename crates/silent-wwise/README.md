# worldrunner-silent-wwise

A Wwise sound engine that plays nothing. It exports the 1170 entry points of Audiokinetic's
Wwise 2021.1 C# binding (`AkSoundEnginePINVOKE`, all `[DllImport("AkSoundEngine")]`) and
returns zero from each one (`false`, null, `0.0`). A game whose audio goes through Wwise, such
as Old World, then starts on a platform where it ships no native `AkSoundEngine` library, and
plays no sound. Real audio is later work.

`cargo build -p worldrunner-silent-wwise` builds `libworldrunner_silent_wwise.so`
(`.dylib` on macOS, `worldrunner_silent_wwise.dll` on Windows) into the Cargo target directory
next to `worldrunner-disunity-ffi`. The player host looks for it there: when a player's
`<Game>_Data/Plugins` directories hold no `AkSoundEngine` library for the platform, the managed
engine loads this one in its place (see `docs/unity-projects.md`, "Native plugins").

The functions take no parameters. The binding declares arguments, but the caller removes them
on every C calling convention in use on the supported platforms, so ignoring them is safe.

## The entry point list

`exports.txt` has one line per entry point, `<return type> <name>`, with the C# binding's
return type (`void`, `int`, `uint`, `IntPtr`, `float`, `byte`, `bool`, `ushort`, `ulong`,
`Vector3`). `build.rs` generates a snake-case `extern "C"` function for each line into
`OUT_DIR`; `bool` is a 4-byte `BOOL` under default P/Invoke marshalling, so it returns `i32`.

To regenerate the list for another Wwise version, decompile the binding's P/Invoke class and
extract the declarations:

```sh
ilspycmd -t AkSoundEnginePINVOKE <Game>_Data/Managed/AK.Wwise.Unity.API.dll > pinvoke.cs
grep -h "static extern" pinvoke.cs \
  | sed -E 's/^[[:space:]]*public static extern ([A-Za-z0-9_]+) ([A-Za-z0-9_]+)\(.*/\1 \2/' \
  > crates/silent-wwise/exports.txt
```

The build fails with the line number of any line it cannot parse, and the unit tests pin the
entry count, so a changed list needs its count updated in `src/lib.rs`.
