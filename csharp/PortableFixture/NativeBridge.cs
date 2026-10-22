using System.Runtime.InteropServices;
using Disunity;

namespace PortableFixture;

/// <summary>Original integration fixture: one triangle of native objects with deterministic transforms.</summary>
public static class NativeBridge
{
    private static readonly HashSet<nint> Tokens = new();

    [UnmanagedCallersOnly]
    public static int ConfigureNativeLibrary(nint utf8Path)
    {
        try
        {
            WorldRunner.ConfigureNativeLibrary(Marshal.PtrToStringUTF8(utf8Path)
                ?? throw new ArgumentException("native library path is required"));
            return 0;
        }
        catch { return -1; }
    }

    [UnmanagedCallersOnly]
    public static int Run(nint utf8Fixture)
    {
        try
        {
            var path = Marshal.PtrToStringUTF8(utf8Fixture);
            if (path == null || File.ReadAllText(path).Trim() != "portable-triangle-v1")
            {
                return -2;
            }
            WorldRunner.LoadWorld();
            var behaviours = Enumerable.Range(0, 3)
                .Select(i => WorldRunner.AddComponent(new TriangleBehaviour(i))).ToArray();
            WorldRunner.Step(0.125f);
            if (behaviours.Any(b => b.Starts != 1 || b.Updates != 1 || b.LateUpdates != 1)
                || Time.Frame != 1 || Time.DeltaTime != 0.125f)
            {
                throw new InvalidOperationException("all lifecycle phases must run in the first step");
            }
            return 0;
        }
        catch
        {
            WorldRunner.UnloadWorld();
            return -3;
        }
    }

    [UnmanagedCallersOnly]
    public static ulong WorldHandle() => WorldRunner.WorldHandle;

    [UnmanagedCallersOnly]
    public static void Unload() => WorldRunner.UnloadWorld();

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
        if (Tokens.Remove(token)) { GCHandle.FromIntPtr(token).Free(); }
    }

    [UnmanagedCallersOnly]
    public static int TokenLive() => Tokens.Count;
}
