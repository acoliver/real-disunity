using System.Runtime.InteropServices;
using UnityEngine;
using Disunity.Hosting;

namespace PortableFixture;

/// <summary>Original integration fixture: one triangle of native objects with deterministic transforms.</summary>
public static class NativeBridge
{
    private static readonly HashSet<nint> Tokens = new();
    private static string LastError = "";
    private static int DestroyedBefore;
    private static int AssetsBefore;

    [UnmanagedCallersOnly]
    public static int ConfigureNativeLibrary(nint utf8Path)
    {
        try
        {
            DisunityHost.ConfigureNativeLibrary(Marshal.PtrToStringUTF8(utf8Path)
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
            var kind = path == null ? "" : File.ReadAllText(path).Trim();
            if (kind != "portable-sprites-v1" && kind != "portable-triangle-v1" && kind != "portable-load-failure-v1" && kind != "portable-callback-failure-v1" && kind != "portable-long-failure-v1")
            {
                return -2;
            }
            TriangleBehaviour.FailUpdates = false;
            TriangleBehaviour.LongFailure = kind == "portable-long-failure-v1";
            AssetsBefore = FixtureAsset.Destroyed;
            DestroyedBefore = TriangleBehaviour.Destroyed;
            DisunityHost.LoadWorld();
            var behaviours = Enumerable.Range(0, 3)
                .Select(i => DisunityHost.AddComponent(new TriangleBehaviour(i))).ToArray();
            if (kind == "portable-sprites-v1")
                foreach (var behaviour in behaviours)
                {
                    var renderer = behaviour.gameObject.AddComponent<SpriteRenderer>();
                    renderer.sprite = DisunityHost.CreateSprite(7, 8, 8, new Rect(0, 0, 2, 4), new Vector2(0.5f, 0.5f), 2);
                    renderer.color = new Color(0.2f, 0.4f, 0.6f, 0.5f);
                    renderer.flipX = true;
                    renderer.sortingLayerID = 1;
                    renderer.sortingOrder = 2;
                }
            if (kind == "portable-sprites-v1")
            {
                var camera = DisunityHost.CreateGameObject();
                camera.transform.position = new Vector3(0, 0, -10);
                camera.AddComponent<Camera>();
                var canvas = DisunityHost.CreateGameObject();
                var rect = canvas.AddComponent<RectTransform>();
                canvas.AddComponent<Canvas>();
                var label = DisunityHost.CreateGameObject();
                var child = label.AddComponent<RectTransform>();
                child.SetParent(rect, false);
                child.sizeDelta = new Vector2(30, 20);
                child.anchoredPosition = new Vector2(11, -3);
                var text = DisunityHost.CreateText(label, DisunityHost.RegisterFont(SyntheticFont.Bytes()));
                text.text = "AB";
                text.fontSize = 12;
            }
            if (kind == "portable-load-failure-v1") throw new InvalidOperationException("fixture load failed after allocation");
            DisunityHost.Step(0.125f);
            if (behaviours.Any(b => b.Starts != 1 || b.Updates != 1 || b.LateUpdates != 1)
                || Time.Frame != 1 || Time.DeltaTime != 0.125f)
            {
                throw new InvalidOperationException("all lifecycle phases must run in the first step");
            }
            TriangleBehaviour.FailUpdates = kind is "portable-callback-failure-v1" or "portable-long-failure-v1";
            return 0;
        }
        catch (Exception error)
        {
            LastError = error.ToString();
            try { DisunityHost.UnloadWorld(); } catch (Exception cleanup) { LastError += cleanup.ToString(); }
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
            var loaded = DisunityHost.IsLoaded;
            if (loaded && FixtureAsset.Destroyed != AssetsBefore + 3 && Time.Frame > 1) throw new InvalidOperationException("Rust-driven frame omitted asset teardown");
            DisunityHost.UnloadWorld();
            if (loaded && FixtureAsset.Destroyed != AssetsBefore + 3) throw new InvalidOperationException("live asset teardown omitted");
            if (loaded && !TriangleBehaviour.FailUpdates && TriangleBehaviour.Destroyed != DestroyedBefore + 3)
                throw new InvalidOperationException("native image did not remain available through final callbacks");
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
        if (Tokens.Remove(token)) { GCHandle.FromIntPtr(token).Free(); }
    }

    [UnmanagedCallersOnly]
    public static int TokenLive() => Tokens.Count;
}
