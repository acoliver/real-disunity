using System.Text;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using static GraphicsTestSupport;

/// <summary>A Texture3D on a material or a property block reaches the host as an asset texture under a stored identity, and the host
/// reads back its depth, dimension, format and bytes, which Apply stores again. A volume with a mip chain stores its levels one after
/// another, each with its own slices. A destroyed volume is no texture.</summary>
internal static unsafe class Texture3DWorldTests
{
    private const uint Material = 0, Block = 2, Asset = 1;
    private const uint Tex3D = 3;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            AVolumeOnAMaterialAndAPropertyBlockIsSentAsAnAssetTexture();
            Console.WriteLine("PASS: a Texture3D on a material or a property block is sent as an asset texture with an identity in the player range");
            TheHostReadsBackTheDepthDimensionFormatAndBytes();
            Console.WriteLine("PASS: the host reads back a volume's depth, dimension, graphics format and bytes by its identity");
            AVolumeWithAMipChainStoresEveryLevelWithItsSlices();
            Console.WriteLine("PASS: a volume with a mip chain stores its levels one after another, including one that is not a whole number of equal slices");
            ApplyStoresTheVolumeAgainAndASamplerChangeFollows();
            Console.WriteLine("PASS: Apply after SetPixels32 raises the stored generation, and a sampler change reaches the host");
            ADestroyedVolumeIsNoTexture();
            Console.WriteLine("PASS: a destroyed volume is sent as no texture");
            AVolumeInAFormatWithNoTextureFormatStoresItsRawBytesAndFormat();
            Console.WriteLine("PASS: a volume in a format with no TextureFormat stores its raw bytes and its graphics format");
            AVolumeFromAPlayerCarriesItsPlanAndBytes();
            Console.WriteLine("PASS: a planned volume keeps its size, depth, format and bytes, and a plan that disagrees with them fails by name");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    /// <summary>A 2x1 volume of four slices in RGBA32 whose slice s holds texels (10 s + 1 + bump, s, 7, 255) and (10 s + 2 + bump, s, 7, 255),
    /// with one level unless <paramref name="mipChain"/>.</summary>
    private static Texture3D FourSlices(bool linear = true, bool mipChain = false, int bump = 0)
    {
        var volume = new Texture3D(2, 1, 4, linear ? GraphicsFormat.R8G8B8A8_UNorm : GraphicsFormat.R8G8B8A8_SRGB,
            mipChain ? TextureCreationFlags.MipChain : TextureCreationFlags.None);
        volume.SetPixels32(Texels(bump), 0);
        return volume;
    }

    private static Color32[] Texels(int bump) =>
        Enumerable.Range(0, 4).SelectMany(slice => new[]
        {
            new Color32((byte)(10 * slice + 1 + bump), (byte)slice, 7, 255), new Color32((byte)(10 * slice + 2 + bump), (byte)slice, 7, 255),
        }).ToArray();

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

    /// <summary>The identity a material's slot names for a volume, which must be an asset texture.</summary>
    private static ulong IdentityOnMaterial(UnityEngine.Material material, string property)
    {
        var slot = ReadProperty(Material, material.WorldLink.Id, property);
        Expect.That(slot.present == 16 && slot.texture_kind == Asset, $"{property} is sent as an asset texture, got kind {slot.texture_kind}");
        return slot.texture;
    }

    private static void AVolumeOnAMaterialAndAPropertyBlockIsSentAsAnAssetTexture()
    {
        var volume = FourSlices();
        var material = NewMaterial("Test/Volume/Slot");
        material.SetTexture("_3DNoiseTexture", volume);
        var identity = IdentityOnMaterial(material, "_3DNoiseTexture");
        Expect.That(identity >= DisunityHost.FirstPlayerTextureIdentity, "the identity is in the player range");

        var renderer = DisunityHost.CreateGameObject().AddComponent<MeshRenderer>();
        var block = new MaterialPropertyBlock();
        block.SetTexture("_3DNoiseTexture", volume);
        renderer.SetPropertyBlock(block);
        var inBlock = ReadProperty(Block, renderer.Handle, "_3DNoiseTexture");
        Expect.That(inBlock.texture_kind == Asset && inBlock.texture == identity, "a property block names the same identity as the material, got " + inBlock.texture);

        material.SetTexture("_Other", FourSlices());
        Expect.That(IdentityOnMaterial(material, "_Other") != identity, "a second volume has its own identity");
        Expect.That(IdentityOnMaterial(material, "_3DNoiseTexture") == identity, "the volume keeps its identity");
    }

    private static void TheHostReadsBackTheDepthDimensionFormatAndBytes()
    {
        foreach (var linear in new[] { true, false })
        {
            var material = NewMaterial("Test/Volume/Read");
            material.SetTexture("_3DDirt", FourSlices(linear));
            var (info, bytes) = ReadStored(IdentityOnMaterial(material, "_3DDirt"));
            var graphics = linear ? GraphicsFormat.R8G8B8A8_UNorm : GraphicsFormat.R8G8B8A8_SRGB;
            Expect.That(info.present == 1 && info.generation == 1, "the volume is stored once");
            Expect.That(info.depth == 4 && info.dimension == Tex3D, $"the record has 4 slices of a 3D texture, got {info.depth} and dimension {info.dimension}");
            Expect.That(info.graphics_format == (uint)graphics, $"the record carries the graphics format {graphics}, got {info.graphics_format}");
            Expect.That(info.format == (uint)TextureFormat.RGBA32 && info.width == 2 && info.height == 1 && info.mips == 1, "format, size and mips");
            Expect.That(info.linear == (linear ? 1u : 0u) && info.length == 32, "the colour space and length");
            Expect.Sequence(new byte[]
            {
                1, 0, 7, 255, 2, 0, 7, 255, 11, 1, 7, 255, 12, 1, 7, 255, 21, 2, 7, 255, 22, 2, 7, 255, 31, 3, 7, 255, 32, 3, 7, 255,
            }, bytes, "the stored bytes are the slices one after another");
        }
    }

    private static void AVolumeWithAMipChainStoresEveryLevelWithItsSlices()
    {
        // 2x1x4 voxels with levels of 2x1x4, 1x1x2 and 1x1x1: 32 + 8 + 4 bytes, which four slices do divide.
        var chain = FourSlices(mipChain: true);
        var material = NewMaterial("Test/Volume/Mips");
        material.SetTexture("_3DDirt", chain);
        var (info, bytes) = ReadStored(IdentityOnMaterial(material, "_3DDirt"));
        Expect.That(info.mips == 3 && info.length == 44 && info.depth == 4, $"three levels, 44 bytes and 4 slices, got {info.mips}, {info.length} and {info.depth}");
        Expect.Sequence(chain.PixelBytes.ToArray(), bytes, "the levels are stored one after another");

        // 3x3x3 voxels with levels of 3x3x3 and 1x1x1: 112 bytes, which three slices do not divide.
        var odd = new Texture3D(3, 3, 3, TextureFormat.RGBA32, true) { name = "Odd" };
        odd.SetPixels32(Enumerable.Repeat(new Color32(9, 8, 7, 255), 27).ToArray(), 0);
        odd.Apply();
        var oddMaterial = NewMaterial("Test/Volume/Odd");
        oddMaterial.SetTexture("_3DDirt", odd);
        var (oddInfo, oddBytes) = ReadStored(IdentityOnMaterial(oddMaterial, "_3DDirt"));
        Expect.That(oddInfo.mips == 2 && oddInfo.length == 112 && oddInfo.depth == 3, $"two levels, 112 bytes and 3 slices, got {oddInfo.mips}, {oddInfo.length} and {oddInfo.depth}");
        Expect.Sequence(odd.PixelBytes.ToArray(), oddBytes, "a volume whose chain is not whole slices is stored as it is");
    }

    private static void ApplyStoresTheVolumeAgainAndASamplerChangeFollows()
    {
        var volume = FourSlices();
        var material = NewMaterial("Test/Volume/Apply");
        material.SetTexture("_3DDirt", volume);
        var identity = IdentityOnMaterial(material, "_3DDirt");
        volume.SetPixels32(Texels(5), 0);
        Expect.That(ReadStored(identity).Info.generation == 1, "changing pixels stores nothing until Apply");
        volume.Apply();
        var (info, bytes) = ReadStored(identity);
        Expect.That(info.generation == 2, "Apply stores a volume that has an identity again");
        Expect.That(bytes[0] == 6 && bytes[4] == 7, $"Apply stores the changed pixels, got {bytes[0]} and {bytes[4]}");
        Expect.That(info.depth == 4 && info.dimension == Tex3D, "the record keeps its shape");

        var first = ReadSampler(identity);
        volume.filterMode = FilterMode.Point;
        volume.wrapModeW = TextureWrapMode.Clamp;
        var changed = ReadSampler(identity);
        Expect.That(changed.generation > first.generation && changed.filter == 0 && changed.wrap_w == 1, "a sampler change reaches the host");

        var probe = new Texture2D(1, 1).RendererIdentity;
        FourSlices().Apply();
        Expect.That(new Texture2D(1, 1).RendererIdentity == probe + 1, "Apply on a volume the host never saw gives it no identity");
    }

    private static void ADestroyedVolumeIsNoTexture()
    {
        var doomed = FourSlices();
        var material = NewMaterial("Test/Volume/Destroyed");
        material.SetTexture("_3DDirt", doomed);
        IdentityOnMaterial(material, "_3DDirt");
        UnityEngine.Object.DestroyImmediate(doomed);
        Expect.That(WorldGraphics.TextureOf(doomed) == (0u, 0UL), "a destroyed volume is no texture");
        material.SetTexture("_Gone", doomed);
        Expect.That(ReadProperty(Material, material.WorldLink.Id, "_Gone").texture_kind == 0, "a destroyed volume sent to a slot is no texture");
    }

    private static void AVolumeInAFormatWithNoTextureFormatStoresItsRawBytesAndFormat()
    {
        // As Old World's noise volumes are planned: four bytes a voxel in GraphicsFormat 88, which has no TextureFormat.
        var noise = Texture3D.FromPlayer("Noise", new PlayerTexture3D(2, 2, 2, 88, 1, true, 32));
        for (var i = 0; i < noise.PixelBytes.Length; i++) noise.PixelBytes[i] = (byte)(i + 1);
        Expect.Throws<NotSupportedException>(() => _ = noise.format, "a format without a TextureFormat");
        Expect.Throws<NotSupportedException>(() => noise.GetPixels(), "pixel access without a codec");
        var material = NewMaterial("Test/Volume/Raw");
        material.SetTexture("_3DNoiseTexture", noise);
        var (info, bytes) = ReadStored(IdentityOnMaterial(material, "_3DNoiseTexture"));
        Expect.That(info.format == 0 && info.graphics_format == 88, $"no TextureFormat and graphics format 88, got {info.format} and {info.graphics_format}");
        Expect.That(info.depth == 2 && info.dimension == Tex3D && info.length == 32 && info.linear == 1, "the shape, length and colour space");
        Expect.Sequence(Enumerable.Range(1, 32).Select(i => (byte)i).ToArray(), bytes, "the raw bytes are stored as they are");
    }

    private static void AVolumeFromAPlayerCarriesItsPlanAndBytes()
    {
        // 2x1x4 RGBA32 voxels in sRGB with three levels: 44 bytes.
        var volume = Texture3D.FromPlayer("Dirt", new PlayerTexture3D(2, 1, 4, (int)GraphicsFormat.R8G8B8A8_SRGB, 3, true, 44));
        Expect.That(volume.name == "Dirt" && volume.width == 2 && volume.height == 1 && volume.depth == 4 && volume.mipmapCount == 3, "the planned name, size and levels");
        Expect.That(volume.format == TextureFormat.RGBA32 && volume.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB, "the planned format in sRGB");
        Expect.Equal(44, volume.ByteLength, "the bytes of three levels");
        Expect.Throws<InvalidDataException>(() => Texture3D.FromPlayer("Short", new PlayerTexture3D(2, 1, 4, (int)GraphicsFormat.R8G8B8A8_SRGB, 3, true, 43)), "a plan whose size is not the bytes");
        Expect.Throws<InvalidDataException>(() => Texture3D.FromPlayer("Flat", new PlayerTexture3D(2, 1, 0, (int)GraphicsFormat.R8G8B8A8_SRGB, 1, true, 8)), "a plan with no slices");
    }
}
