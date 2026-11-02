using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using UnityEngine;
using UnityEngine.TextCore;
using UnityEngine.TextCore.LowLevel;
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

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static string? Missing() => null;

    /// <summary>
    /// Reads through a null reference, which .NET detects with a hardware fault and turns into a
    /// NullReferenceException inside the runtime's SIGSEGV handler, and catches it. A host whose
    /// thread keeps a signal stack too small for that handler crashes here instead.
    /// </summary>
    [MethodImpl(MethodImplOptions.NoInlining)]
    private static void CatchHardwareFault()
    {
        try
        {
            _ = Missing()!.Length;
            throw new InvalidOperationException("reading through a null reference did not fault");
        }
        catch (NullReferenceException)
        {
        }
    }

    [UnmanagedCallersOnly]
    public static int Run(nint utf8Fixture)
    {
        try
        {
            var path = Marshal.PtrToStringUTF8(utf8Fixture);
            var kind = path == null ? "" : File.ReadAllText(path).Trim();
            if (kind is not ("portable-lit-sprites-v1" or "portable-sprites-v1" or "portable-triangle-v1" or "portable-load-failure-v1" or "portable-callback-failure-v1" or "portable-long-failure-v1" or "portable-perspective-v1" or "portable-camera-rect-v1" or "portable-texture-v1" or "portable-atlas-v1" or "portable-mesh-v1"))
            {
                return -2;
            }
            CatchHardwareFault();
            TriangleBehaviour.FailUpdates = false;
            TriangleBehaviour.LongFailure = kind == "portable-long-failure-v1";
            AssetsBefore = FixtureAsset.Destroyed;
            DestroyedBefore = TriangleBehaviour.Destroyed;
            DisunityHost.LoadWorld();
            var behaviours = Enumerable.Range(0, 3)
                .Select(i => DisunityHost.AddComponent(new TriangleBehaviour(i))).ToArray();
            if (kind is "portable-sprites-v1" or "portable-lit-sprites-v1") AddSprites(behaviours, kind);
            if (kind == "portable-sprites-v1") AddCameraUi();
            if (kind == "portable-lit-sprites-v1") SpriteLightPulse.Create();
            if (kind is "portable-perspective-v1" or "portable-camera-rect-v1") AddPerspectiveCamera(kind == "portable-camera-rect-v1");
            if (kind == "portable-texture-v1") AddTexture();
            if (kind == "portable-atlas-v1") AddAtlas();
            if (kind == "portable-mesh-v1") AddMesh();
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

    private static void AddSprites(TriangleBehaviour[] behaviours, string kind)
    {
        foreach (var behaviour in behaviours)
        {
            var renderer = behaviour.gameObject.AddComponent<SpriteRenderer>();
            renderer.sprite = DisunityHost.CreateSprite(7, 8, 8, new Rect(0, 0, 2, 4), new Vector2(0.5f, 0.5f), 2);
            renderer.color = new Color(0.2f, 0.4f, 0.6f, 0.5f);
            renderer.flipX = true;
            renderer.sortingLayerID = 1;
            renderer.sortingOrder = 2;
            if (kind == "portable-lit-sprites-v1") renderer.sharedMaterial = Material.SpriteLit;
        }
    }
    /// <summary>A two pixel texture that takes its renderer identity, (a sprite is made over it), so the host can read its bytes.</summary>
    private static void AddTexture()
    {
        var texture = new Texture2D(2, 1, TextureFormat.RGBA32, false, true);
        texture.SetPixel(0, 0, new Color32(9, 8, 7, 6));
        texture.SetPixel(1, 0, new Color32(5, 4, 3, 2));
        Sprite.Create(texture, new Rect(0, 0, 2, 1), new Vector2(0.5f, 0.5f));
    }
    /// <summary>An Alpha8 atlas that takes its renderer identity and then has a distance field glyph drawn into it, as TextMesh Pro does.</summary>
    private static void AddAtlas()
    {
        var atlas = new Texture2D(64, 64, TextureFormat.Alpha8, false, true);
        Sprite.Create(atlas, new Rect(0, 0, 64, 64), new Vector2(0.5f, 0.5f));
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 40);
        var free = new List<GlyphRect> { new(0, 0, 64, 64) };
        FontEngine.TryAddGlyphToTexture(1, 4, GlyphPackingMode.BestShortSideFit, free,
            new List<GlyphRect>(), GlyphRenderMode.SDFAA, atlas, out _);
    }
    /// <summary>A scripted quad of two sub-meshes, one material and a position, so the host can read the mesh and where it is drawn.</summary>
    private static void AddMesh()
    {
        var shown = DisunityHost.CreateGameObject();
        shown.transform.position = new Vector3(3, 0, 2);
        var mesh = new Mesh
        {
            vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(1, 1, 0), new Vector3(0, 1, 0)],
            uv = [new Vector2(0, 0), new Vector2(1, 0), new Vector2(1, 1), new Vector2(0, 1)],
            subMeshCount = 2,
        };
        mesh.SetTriangles(new[] { 0, 2, 1 }, 0);
        mesh.SetTriangles(new[] { 0, 3, 2 }, 1);
        shown.AddComponent<MeshFilter>().sharedMesh = mesh;
        var material = new Material(DisunityHost.RegisterShader("Fixture/Plain"));
        material.SetColor("_Color", new Color(0.25f, 0.5f, 0.75f, 1f));
        shown.AddComponent<MeshRenderer>().sharedMaterials = [material, null];
    }
    private static void AddPerspectiveCamera(bool partialRect)
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.orthographic = false;
        camera.fieldOfView = 90;
        camera.nearClipPlane = 0.5f;
        camera.farClipPlane = 200;
        if (partialRect) camera.rect = new Rect(0, 0, 0.5f, 1);
    }
    private static void AddCameraUi()
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
            xyz[0] = value.x; xyz[1] = value.y; xyz[2] = value.z;
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
