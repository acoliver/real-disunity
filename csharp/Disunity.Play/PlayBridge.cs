using System.Runtime.InteropServices;
using System.Runtime.Loader;
using System.Text.Json;
using Disunity.Hosting;

namespace Disunity.Play;

/// <summary>
/// The managed side of the play harness. <c>Run</c> loads the scene a play request names and
/// returns; the Rust host then advances the world, delivers input and renders each frame.
/// A request is JSON: <c>{"plan": path, "scripts": path, "fonts": {"Arial": path}, "first_texture": 1}</c>.
/// </summary>
public static class PlayBridge
{
    private static readonly HashSet<nint> Tokens = new();
    private static string LastError = "";
    private static LoadedScene? Scene;

    [UnmanagedCallersOnly]
    public static int ConfigureNativeLibrary(nint utf8Path)
    {
        try
        {
            DisunityHost.ConfigureNativeLibrary(Marshal.PtrToStringUTF8(utf8Path)
                ?? throw new ArgumentException("native library path is required"));
            return 0;
        }
        catch (Exception error) { LastError = error.ToString(); return -1; }
    }

    [UnmanagedCallersOnly]
    public static int Run(nint utf8Request)
    {
        try
        {
            var path = Marshal.PtrToStringUTF8(utf8Request) ?? throw new ArgumentException("play request path is required");
            using var request = JsonDocument.Parse(File.ReadAllText(path));
            var root = request.RootElement;
            var fonts = root.GetProperty("fonts").EnumerateObject().ToDictionary(font => font.Name, font => font.Value.GetString()!);
            // The host loads this bridge into its own context; the game must bind the same
            // Disunity.Engine, so it loads there too.
            var context = AssemblyLoadContext.GetLoadContext(typeof(PlayBridge).Assembly)!;
            var scripts = context.LoadFromAssemblyPath(Path.GetFullPath(root.GetProperty("scripts").GetString()!));
            var options = new SceneLoadOptions(scripts, name =>
                fonts.TryGetValue(name, out var file) ? File.ReadAllBytes(file) : throw new FileNotFoundException($"no font file for {name}"))
            {
                FirstTextureIdentity = root.GetProperty("first_texture").GetUInt64(),
            };
            DisunityHost.LoadWorld();
            Scene = DisunityHost.LoadScene(File.ReadAllText(root.GetProperty("plan").GetString()!), options);
            foreach (var line in Scene.UnregisteredAxes) Console.Error.WriteLine($"input axis not registered: {line}");
            foreach (var line in Scene.Differences) Console.Error.WriteLine($"not reproduced: {line}");
            return 0;
        }
        catch (Exception error)
        {
            LastError = error.ToString();
            try { if (DisunityHost.IsLoaded) DisunityHost.UnloadWorld(); }
            catch (Exception cleanup) { LastError += cleanup.ToString(); }
            return -3;
        }
    }

    [UnmanagedCallersOnly]
    public static ulong WorldHandle() => DisunityHost.WorldHandle;

    [UnmanagedCallersOnly]
    public static int Unload()
    {
        try
        {
            Scene?.Dispose();
            Scene = null;
            DisunityHost.UnloadWorld();
            return 0;
        }
        catch (Exception error) { LastError = error.ToString(); return -1; }
    }

    [UnmanagedCallersOnly]
    public static int Dispatch(ulong identity, uint message)
    {
        try { DisunityHost.Dispatch(identity, message); return 0; }
        catch (Exception error) { DisunityHost.ReportFault(); LastError = error.ToString(); return -1; }
    }

    [UnmanagedCallersOnly]
    public static unsafe int Error(byte* output, uint capacity, uint* written)
    {
        var bytes = System.Text.Encoding.UTF8.GetBytes(LastError);
        *written = checked((uint)bytes.Length);
        if (capacity < bytes.Length) return 2;
        bytes.CopyTo(new Span<byte>(output, (int)capacity));
        return 0;
    }

    [UnmanagedCallersOnly]
    public static unsafe int ReadPosition(ulong handle, float* xyz)
    {
        try
        {
            var value = DisunityHost.ObjectPosition(handle);
            xyz[0] = value.X; xyz[1] = value.Y; xyz[2] = value.Z;
            return 0;
        }
        catch (Exception error) { LastError = error.ToString(); return -1; }
    }

    [UnmanagedCallersOnly]
    public static nint TokenAlloc()
    {
        var token = GCHandle.ToIntPtr(GCHandle.Alloc(new object()));
        Tokens.Add(token);
        return token;
    }

    [UnmanagedCallersOnly]
    public static void TokenFree(nint token)
    {
        if (Tokens.Remove(token)) GCHandle.FromIntPtr(token).Free();
    }

    [UnmanagedCallersOnly]
    public static int TokenLive() => Tokens.Count;
}
