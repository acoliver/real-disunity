using System.Text;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using static GraphicsTestSupport;

/// <summary>A Texture2DArray on a material or a property block reaches the host as an asset texture under a stored identity,
/// and the host reads back its layers, dimension, format and bytes, which Apply stores again. A destroyed array is no texture, and
/// so is one too large for the host to store.</summary>
internal static unsafe class Texture2DArrayWorldTests
{
    private const uint Material = 0, Block = 2, Asset = 1;
    private const uint Tex2DArray = 5;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            AnArrayOnAMaterialAndAPropertyBlockIsSentAsAnAssetTexture();
            Console.WriteLine("PASS: a Texture2DArray on a material or a property block is sent as an asset texture with an identity in the player range");
            TheHostReadsBackTheLayersDimensionFormatAndBytes();
            Console.WriteLine("PASS: the host reads back an array's layers, dimension, graphics format and bytes by its identity");
            ApplyStoresTheArrayAgainAndASamplerChangeFollows();
            Console.WriteLine("PASS: Apply after SetPixels raises the stored generation, and a sampler change reaches the host");
            ADestroyedArrayIsNoTexture();
            Console.WriteLine("PASS: a destroyed array is sent as no texture");
            AnUnmappedFormatStillStoresItsRawBytesAndFormat();
            Console.WriteLine("PASS: an array in a format with no TextureFormat stores its raw bytes and its graphics format");
            AnArrayTheHostCannotStoreIsNoTextureAndIsNamed();
            Console.WriteLine("PASS: an array larger than the host stores or not made of whole layers is sent as no texture and named in a warning");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    /// <summary>A 2x1 array of three layers in RGBA32 whose layer s holds texels (10 s + 1, s, 7, 255) and (10 s + 2, s, 7, 255).</summary>
    private static Texture2DArray ThreeLayers(bool linear = true)
    {
        var array = new Texture2DArray(2, 1, 3, TextureFormat.RGBA32, false, linear);
        for (var layer = 0; layer < 3; layer++)
            array.SetPixels32(Texels(layer, 0), layer);
        return array;
    }

    private static Color32[] Texels(int layer, int bump) =>
        [new Color32((byte)(10 * layer + 1 + bump), (byte)layer, 7, 255), new Color32((byte)(10 * layer + 2 + bump), (byte)layer, 7, 255)];

    private static byte[] ThreeLayerBytes(int bump) =>
        Enumerable.Range(0, 3).SelectMany(layer => Texels(layer, bump).SelectMany(c => new[] { c.r, c.g, c.b, c.a })).ToArray();

    private static UnityEngine.Material NewMaterial(string shader) => new(DisunityHost.RegisterShader(shader));

    private static DisunityPropertyRead ReadProperty(uint scope, ulong target, string name)
    {
        var bytes = Encoding.UTF8.GetBytes(name);
        DisunityPropertyRead value;
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_shader_property_read(DisunityHost.WorldHandle, scope, target, pointer, (uint)bytes.Length, &value));
        return value;
    }

    private static (DisunityTextureInfo Info, byte[] Bytes) ReadStored(ulong identity)
    {
        var info = new DisunityTextureInfo();
        DisunityHost.ThrowIfError(NativeBindings.disunity_texture_read(DisunityHost.WorldHandle, identity, &info, null, 0));
        var bytes = new byte[info.length];
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_texture_read(DisunityHost.WorldHandle, identity, &info, pointer, info.length));
        return (info, bytes);
    }

    private static DisunitySampler ReadSampler(ulong identity)
    {
        var sampler = new DisunitySampler();
        DisunityHost.ThrowIfError(NativeBindings.disunity_texture_sampler_read(DisunityHost.WorldHandle, identity, &sampler));
        return sampler;
    }

    /// <summary>The identity a material's slot names for an array, which must be an asset texture.</summary>
    private static ulong IdentityOnMaterial(UnityEngine.Material material, string property)
    {
        var slot = ReadProperty(Material, material.WorldLink.Id, property);
        Expect.That(slot.present == 16 && slot.texture_kind == Asset, $"{property} is sent as an asset texture, got kind {slot.texture_kind}");
        return slot.texture;
    }

    private static void AnArrayOnAMaterialAndAPropertyBlockIsSentAsAnAssetTexture()
    {
        var array = ThreeLayers();
        var material = NewMaterial("Test/Array/Slot");
        material.SetTexture("_Atlas", array);
        var identity = IdentityOnMaterial(material, "_Atlas");
        Expect.That(identity >= DisunityHost.FirstPlayerTextureIdentity, "the identity is in the player range");

        var renderer = DisunityHost.CreateGameObject().AddComponent<MeshRenderer>();
        var block = new MaterialPropertyBlock();
        block.SetTexture("_Atlas", array);
        renderer.SetPropertyBlock(block);
        var inBlock = ReadProperty(Block, renderer.Handle, "_Atlas");
        Expect.That(inBlock.texture_kind == Asset && inBlock.texture == identity, "a property block names the same identity as the material, got " + inBlock.texture);

        var other = ThreeLayers();
        material.SetTexture("_Other", other);
        Expect.That(IdentityOnMaterial(material, "_Other") != identity, "a second array has its own identity");
        Expect.That(IdentityOnMaterial(material, "_Atlas") == identity, "the array keeps its identity");
    }

    private static void TheHostReadsBackTheLayersDimensionFormatAndBytes()
    {
        foreach (var linear in new[] { true, false })
        {
            var material = NewMaterial("Test/Array/Read");
            material.SetTexture("_Atlas", ThreeLayers(linear));
            var (info, bytes) = ReadStored(IdentityOnMaterial(material, "_Atlas"));
            var graphics = linear ? GraphicsFormat.R8G8B8A8_UNorm : GraphicsFormat.R8G8B8A8_SRGB;
            Expect.That(info.present == 1 && info.generation == 1, "the array is stored once");
            Expect.That(info.depth == 3 && info.dimension == Tex2DArray, $"the record has 3 layers of a 2D array, got {info.depth} and dimension {info.dimension}");
            Expect.That(info.graphics_format == (uint)graphics, $"the record carries the graphics format {graphics}, got {info.graphics_format}");
            Expect.That(info.format == (uint)TextureFormat.RGBA32 && info.width == 2 && info.height == 1 && info.mips == 1, "format, size and mips");
            Expect.That(info.linear == (linear ? 1u : 0u) && info.length == 24, "the colour space and length");
            Expect.Sequence(ThreeLayerBytes(0), bytes, "the stored bytes are the layers one after another");
        }
    }

    private static void ApplyStoresTheArrayAgainAndASamplerChangeFollows()
    {
        var array = ThreeLayers();
        var material = NewMaterial("Test/Array/Apply");
        material.SetTexture("_Atlas", array);
        var identity = IdentityOnMaterial(material, "_Atlas");
        array.SetPixels32(Texels(1, 5), 1);
        Expect.That(ReadStored(identity).Info.generation == 1, "changing pixels stores nothing until Apply");
        array.Apply();
        var (info, bytes) = ReadStored(identity);
        Expect.That(info.generation == 2, "Apply stores an array that has an identity again");
        Expect.That(bytes[8] == 16 && bytes[12] == 17, $"Apply stores the changed layer, got {bytes[8]} and {bytes[12]}");
        Expect.That(info.depth == 3 && info.dimension == Tex2DArray, "the record keeps its shape");

        var first = ReadSampler(identity);
        array.filterMode = FilterMode.Point;
        array.wrapModeW = TextureWrapMode.Clamp;
        var changed = ReadSampler(identity);
        Expect.That(changed.generation > first.generation && changed.filter == 0 && changed.wrap_w == 1, "a sampler change reaches the host");

        var probe = new Texture2D(1, 1).RendererIdentity;
        ThreeLayers().Apply();
        Expect.That(new Texture2D(1, 1).RendererIdentity == probe + 1, "Apply on an array the host never saw gives it no identity");
    }

    private static void ADestroyedArrayIsNoTexture()
    {
        var doomed = ThreeLayers();
        var material = NewMaterial("Test/Array/Destroyed");
        material.SetTexture("_Atlas", doomed);
        IdentityOnMaterial(material, "_Atlas");
        UnityEngine.Object.DestroyImmediate(doomed);
        Expect.That(WorldGraphics.TextureOf(doomed) == (0u, 0UL), "a destroyed array is no texture");
        material.SetTexture("_Gone", doomed);
        Expect.That(ReadProperty(Material, material.WorldLink.Id, "_Gone").texture_kind == 0, "a destroyed array sent to a slot is no texture");
    }

    private static void AnUnmappedFormatStillStoresItsRawBytesAndFormat()
    {
        var packed = Texture2DArray.FromPlayer("Packed", new PlayerTextureArray(2, 1, 3, 87, 1, true, 42));
        for (var i = 0; i < packed.PixelBytes.Length; i++) packed.PixelBytes[i] = (byte)(i + 1);
        var material = NewMaterial("Test/Array/Raw");
        material.SetTexture("_Packed", packed);
        var (info, bytes) = ReadStored(IdentityOnMaterial(material, "_Packed"));
        Expect.That(info.format == 0 && info.graphics_format == 87, $"no TextureFormat and graphics format 87, got {info.format} and {info.graphics_format}");
        Expect.That(info.depth == 3 && info.dimension == Tex2DArray && info.length == 42, "the shape and length");
        Expect.Sequence(Enumerable.Range(1, 42).Select(i => (byte)i).ToArray(), bytes, "the raw bytes are stored as they are");
    }

    private static void AnArrayTheHostCannotStoreIsNoTextureAndIsNamed()
    {
        // A plan's raw size need not be a whole number of layers: 40 bytes are not 3 layers.
        var uneven = Texture2DArray.FromPlayer("Uneven", new PlayerTextureArray(2, 1, 3, 87, 1, true, 40));
        using (var unevenLog = new LogRecorder())
        {
            var odd = NewMaterial("Test/Array/Uneven");
            odd.SetTexture("_Atlas", uneven);
            Expect.That(ReadProperty(Material, odd.WorldLink.Id, "_Atlas").texture_kind == 0, "an array that is not whole layers is no texture");
            Expect.That(unevenLog.Messages.Any(m => m.Contains("'Uneven'") && m.Contains("not 3 equal layers")), "the warning names the array and the layers");
        }

        // 9 layers of 4096 x 4096 RGBA32 hold 576 MiB, above the 512 MiB the host stores.
        var huge = new Texture2DArray(4096, 4096, 9, TextureFormat.RGBA32, false, true) { name = "Huge" };
        using var log = new LogRecorder();
        var material = NewMaterial("Test/Array/Huge");
        var probe = new Texture2D(1, 1).RendererIdentity;
        material.SetTexture("_Atlas", huge);
        Expect.That(ReadProperty(Material, material.WorldLink.Id, "_Atlas").texture_kind == 0, "an array beyond the host's limit is no texture");
        Expect.That(new Texture2D(1, 1).RendererIdentity == probe + 1, "it takes no identity");
        Expect.That(log.Messages.Any(m => m.Contains("'Huge'") && m.Contains("more than the host stores")), "the warning names the array");
    }
}
