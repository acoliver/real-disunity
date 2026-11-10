using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

/// <summary>A texture's wrap modes, filter, anisotropy and mip bias reach the host with the texture, and again when a setter changes them.</summary>
internal static class PlayerSamplerTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            StoresTheSamplerWithTheTexture();
            UpdatesAStoredTextureWhenASetterChangesIt();
            GivesNoIdentityToATextureASetterChanges();
            SendsTheSamplerOfARenderTexture();
            SendsTheShapeOfARenderTexture();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static unsafe DisunitySampler Read(ulong identity)
    {
        var sampler = new DisunitySampler();
        DisunityHost.ThrowIfError(NativeBindings.disunity_texture_sampler_read(DisunityHost.WorldHandle, identity, &sampler));
        return sampler;
    }

    private static unsafe DisunityRenderTexture ReadRenderTexture(ulong id)
    {
        DisunityRenderTexture desc;
        DisunityHost.ThrowIfError(NativeBindings.disunity_render_texture_read(DisunityHost.WorldHandle, id, &desc));
        return desc;
    }

    private static void StoresTheSamplerWithTheTexture()
    {
        var plain = new Texture2D(2, 2);
        var defaults = Read(plain.RendererIdentity);
        Expect.That(defaults.present == 1 && defaults.wrap_u == 0 && defaults.wrap_v == 0 && defaults.wrap_w == 0 && defaults.filter == 1 && defaults.aniso_level == 1,
            "a texture nobody configured stores repeat, bilinear and anisotropy 1");
        var configured = new Texture2D(2, 2)
        {
            wrapModeU = TextureWrapMode.Clamp, wrapModeV = TextureWrapMode.Mirror, wrapModeW = TextureWrapMode.MirrorOnce,
            filterMode = FilterMode.Trilinear, anisoLevel = 8, mipMapBias = -0.5f,
        };
        var stored = Read(configured.RendererIdentity);
        Expect.That(stored.wrap_u == 1 && stored.wrap_v == 2 && stored.wrap_w == 3, "the wrap modes are stored per axis");
        Expect.That(stored.filter == 2 && stored.aniso_level == 8 && stored.mip_bias == -0.5f, "the filter, anisotropy and mip bias are stored");
        Console.WriteLine("PASS: a texture stores its sampler when it takes a renderer identity");
    }

    private static void UpdatesAStoredTextureWhenASetterChangesIt()
    {
        var texture = new Texture2D(2, 2);
        var identity = texture.RendererIdentity;
        var before = Read(identity).generation;
        texture.wrapModeU = TextureWrapMode.Clamp;
        var changed = Read(identity);
        Expect.That(changed.wrap_u == 1 && changed.wrap_v == 0 && changed.generation == before + 1, "setting wrapModeU changes the stored sampler's u wrap alone");
        texture.wrapMode = TextureWrapMode.Mirror;
        var all = Read(identity);
        Expect.That(all.wrap_u == 2 && all.wrap_v == 2 && all.wrap_w == 2, "wrapMode sets all three axes");
        texture.filterMode = FilterMode.Point;
        texture.anisoLevel = 16;
        texture.mipMapBias = 1.5f;
        var rest = Read(identity);
        Expect.That(rest.filter == 0 && rest.aniso_level == 16 && rest.mip_bias == 1.5f, "filterMode, anisoLevel and mipMapBias reach the stored sampler");
        var generation = rest.generation;
        texture.filterMode = FilterMode.Point;
        Expect.That(Read(identity).generation == generation, "setting a value it already holds changes nothing");
        texture.mipMapBias = float.NaN;
        texture.anisoLevel = 100;
        texture.filterMode = (FilterMode)7;
        var clamped = Read(identity);
        Expect.That(clamped.mip_bias == 0 && clamped.aniso_level == 16 && clamped.filter == 2, "values the host cannot hold are clamped, and a bias that is not a number is 0");
        texture.filterMode = FilterMode.Point;
        texture.anisoLevel = 16;
        texture.mipMapBias = 1.5f;
        texture.Apply();
        var applied = Read(identity);
        Expect.That(applied.wrap_u == 2 && applied.filter == 0, "storing the texture again keeps its sampler");
        Console.WriteLine("PASS: a setter on a stored texture updates its sampler");
    }

    private static void GivesNoIdentityToATextureASetterChanges()
    {
        var probe = new Texture2D(2, 2).RendererIdentity;
        new Texture2D(2, 2).wrapModeU = TextureWrapMode.Clamp;
        Expect.That(new Texture2D(2, 2).RendererIdentity == probe + 1, "a setter on a texture without an identity gives it none");
        var late = new Texture2D(2, 2) { filterMode = FilterMode.Point };
        Expect.That(Read(late.RendererIdentity).filter == 0, "a value set before the identity is stored with the texture");
        Console.WriteLine("PASS: setters before the first store only keep the value for the store");
    }

    private static void SendsTheShapeOfARenderTexture()
    {
        var plain = ReadRenderTexture(new RenderTexture(16, 16, 0).WorldId);
        Expect.That(plain.dimension == 2 && plain.volume_depth == 1, "a plain render texture is one 2D slice");
        var array = new RenderTextureDescriptor(8, 4, RenderTextureFormat.RFloat, 0) { dimension = TextureDimension.Tex2DArray, volumeDepth = 4 };
        var layers = ReadRenderTexture(new RenderTexture(array).WorldId);
        Expect.That(layers.dimension == 5 && layers.volume_depth == 4, "a 2D array keeps its layers");
        var volume = new RenderTexture(8, 8, 0) { dimension = TextureDimension.Tex3D, volumeDepth = 6 };
        var slices = ReadRenderTexture(volume.WorldId);
        Expect.That(slices.dimension == 3 && slices.volume_depth == 6, "a 3D texture keeps its slices");
        var cube = new RenderTexture(8, 8, 0) { dimension = TextureDimension.Cube, volumeDepth = 6 };
        var faces = ReadRenderTexture(cube.WorldId);
        Expect.That(faces.dimension == 4 && faces.volume_depth == 1, "a cube sends one slice whatever its volume depth says");
        Expect.Throws<ArgumentException>(() => new RenderTexture(new RenderTextureDescriptor(8, 8) { dimension = TextureDimension.None }), "a render texture of no dimension");
        Console.WriteLine("PASS: a render texture sends its dimension and volume depth, one slice for 2D and cube");
    }

    private static void SendsTheSamplerOfARenderTexture()
    {
        var target = new RenderTexture(16, 16, 0);
        var first = ReadRenderTexture(target.WorldId);
        Expect.That(first.wrap_u == 1 && first.wrap_v == 1 && first.wrap_w == 1 && first.filter == 1 && first.aniso_level == 1,
            "a render texture starts clamped and bilinear");
        target.filterMode = FilterMode.Trilinear;
        target.wrapModeV = TextureWrapMode.Repeat;
        target.anisoLevel = 4;
        target.mipMapBias = 0.25f;
        var changed = ReadRenderTexture(target.WorldId);
        Expect.That(changed.filter == 2 && changed.wrap_v == 0 && changed.wrap_u == 1 && changed.aniso_level == 4 && changed.mip_bias == 0.25f,
            "RenderTexture filterMode, wrapModeV, anisoLevel and mipMapBias reach the world");
        Expect.That(changed.generation > first.generation, "a sampler change raises the render texture's generation");
        var copy = new RenderTexture(target);
        var copied = ReadRenderTexture(copy.WorldId);
        Expect.That(copied.filter == 2 && copied.wrap_v == 0 && copied.aniso_level == 4, "a copied render texture keeps the sampler");
        Console.WriteLine("PASS: a render texture's sampler reaches the world and follows its setters");
    }
}
