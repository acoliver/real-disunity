using System.Text;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>A Cubemap on a material or a property block reaches the host as an asset texture under a stored identity, and the host
/// reads back its six faces, dimension, format and bytes, with the levels of each face after it. A destroyed cubemap is no texture,
/// and so is one too large for the host to store.</summary>
internal static unsafe class CubemapWorldTests
{
    private const uint Material = 0, Block = 2, Asset = 1;
    private const uint Cube = 4;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ACubemapOnAMaterialAndAPropertyBlockIsSentAsAnAssetTexture();
            Console.WriteLine("PASS: a Cubemap on a material or a property block is sent as an asset texture with an identity in the player range");
            TheHostReadsBackTheFacesDimensionFormatAndBytes();
            Console.WriteLine("PASS: the host reads back a cubemap's six faces, dimension, graphics format and bytes by its identity");
            TheLevelsOfEachFaceTravelWithItsBytes();
            Console.WriteLine("PASS: a cubemap with a mip chain stores each face with all its levels");
            ASamplerChangeReachesTheHost();
            Console.WriteLine("PASS: a cubemap's sampler change reaches the host");
            ADestroyedCubemapIsNoTexture();
            Console.WriteLine("PASS: a destroyed cubemap is sent as no texture");
            ACubemapTheHostCannotStoreIsNoTextureAndIsNamed();
            Console.WriteLine("PASS: a cubemap larger than the host stores is sent as no texture and named in a warning");
            ACubemapFromAPlayerCarriesItsPlanAndBytes();
            Console.WriteLine("PASS: a planned cubemap keeps its format, colour space, size and bytes, and a plan that disagrees with them fails by name");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    /// <summary>A 2x2 cube of RGBA32 texels in linear colour with one level, whose face f holds texels (10 f + 1, f, 7, 255) four times.</summary>
    private static Cubemap SixFaces(bool mipChain = false, bool linear = true)
    {
        var cube = new Cubemap(2, linear ? GraphicsFormat.R8G8B8A8_UNorm : GraphicsFormat.R8G8B8A8_SRGB,
            mipChain ? TextureCreationFlags.MipChain : TextureCreationFlags.None);
        var bytes = cube.PixelBytes;
        var face = bytes.Length / 6;
        for (var f = 0; f < 6; f++)
            for (var texel = 0; texel < face / 4; texel++)
            {
                // The second level of a face holds its first texel's colour plus 100 in red.
                var level = texel >= 4 ? 100 : 0;
                bytes[f * face + texel * 4] = (byte)(10 * f + 1 + level);
                bytes[f * face + texel * 4 + 1] = (byte)f;
                bytes[f * face + texel * 4 + 2] = 7;
                bytes[f * face + texel * 4 + 3] = 255;
            }
        return cube;
    }

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

    /// <summary>The identity a material's slot names for a cubemap, which must be an asset texture.</summary>
    private static ulong IdentityOnMaterial(UnityEngine.Material material, string property)
    {
        var slot = ReadProperty(Material, material.WorldLink.Id, property);
        Expect.That(slot.present == 16 && slot.texture_kind == Asset, $"{property} is sent as an asset texture, got kind {slot.texture_kind}");
        return slot.texture;
    }

    private static void ACubemapOnAMaterialAndAPropertyBlockIsSentAsAnAssetTexture()
    {
        var cube = SixFaces();
        var material = NewMaterial("Test/Cube/Slot");
        material.SetTexture("_SkyCubemap", cube);
        var identity = IdentityOnMaterial(material, "_SkyCubemap");
        Expect.That(identity >= DisunityHost.FirstPlayerTextureIdentity, "the identity is in the player range");

        var renderer = DisunityHost.CreateGameObject().AddComponent<MeshRenderer>();
        var block = new MaterialPropertyBlock();
        block.SetTexture("_SkyCubemap", cube);
        renderer.SetPropertyBlock(block);
        var inBlock = ReadProperty(Block, renderer.Handle, "_SkyCubemap");
        Expect.That(inBlock.texture_kind == Asset && inBlock.texture == identity, "a property block names the same identity as the material, got " + inBlock.texture);

        material.SetTexture("_Other", SixFaces());
        Expect.That(IdentityOnMaterial(material, "_Other") != identity, "a second cubemap has its own identity");
        Expect.That(IdentityOnMaterial(material, "_SkyCubemap") == identity, "the cubemap keeps its identity");
    }

    private static void TheHostReadsBackTheFacesDimensionFormatAndBytes()
    {
        foreach (var linear in new[] { true, false })
        {
            var cube = SixFaces(linear: linear);
            var material = NewMaterial("Test/Cube/Read");
            material.SetTexture("_SkyCubemap", cube);
            var (info, bytes) = ReadStored(IdentityOnMaterial(material, "_SkyCubemap"));
            var graphics = linear ? GraphicsFormat.R8G8B8A8_UNorm : GraphicsFormat.R8G8B8A8_SRGB;
            Expect.That(info.present == 1 && info.generation == 1, "the cubemap is stored once");
            Expect.That(info.depth == 6 && info.dimension == Cube, $"the record has 6 faces of a cube, got {info.depth} and dimension {info.dimension}");
            Expect.That(info.graphics_format == (uint)graphics, $"the record carries the graphics format {graphics}, got {info.graphics_format}");
            Expect.That(info.format == (uint)TextureFormat.RGBA32 && info.width == 2 && info.height == 2 && info.mips == 1, "format, size and mips");
            Expect.That(info.linear == (linear ? 1u : 0u) && info.length == 96, "the colour space and length");
            Expect.Sequence(cube.PixelBytes.ToArray(), bytes, "the stored bytes are the faces one after another");
            Expect.That(bytes[0] == 1 && bytes[16] == 11 && bytes[80] == 51, "face f starts at 16 f with red 10 f + 1");
        }
    }

    private static void TheLevelsOfEachFaceTravelWithItsBytes()
    {
        var cube = SixFaces(mipChain: true);
        var material = NewMaterial("Test/Cube/Mips");
        material.SetTexture("_SkyCubemap", cube);
        var (info, bytes) = ReadStored(IdentityOnMaterial(material, "_SkyCubemap"));
        Expect.That(info.mips == 2 && info.length == 120, $"two levels a face, 16 + 4 bytes, got {info.mips} levels and {info.length} bytes");
        Expect.Sequence(cube.PixelBytes.ToArray(), bytes, "each face is followed by its second level before the next face");
        Expect.That(bytes[20] == 11 && bytes[16] == 101, "the second face starts after the first face's second level");
    }

    private static void ASamplerChangeReachesTheHost()
    {
        var cube = SixFaces();
        var material = NewMaterial("Test/Cube/Sampler");
        material.SetTexture("_SkyCubemap", cube);
        var identity = IdentityOnMaterial(material, "_SkyCubemap");
        var first = ReadSampler(identity);
        cube.filterMode = FilterMode.Point;
        cube.wrapModeU = TextureWrapMode.Clamp;
        var changed = ReadSampler(identity);
        Expect.That(changed.generation > first.generation && changed.filter == 0 && changed.wrap_u == 1, "a sampler change reaches the host");
    }

    private static void ADestroyedCubemapIsNoTexture()
    {
        var doomed = SixFaces();
        var material = NewMaterial("Test/Cube/Destroyed");
        material.SetTexture("_SkyCubemap", doomed);
        IdentityOnMaterial(material, "_SkyCubemap");
        UnityEngine.Object.DestroyImmediate(doomed);
        Expect.That(WorldGraphics.TextureOf(doomed) == (5u, 0UL), "a destroyed cubemap is named and not sent");
        material.SetTexture("_Gone", doomed);
        Expect.That(ReadProperty(Material, material.WorldLink.Id, "_Gone").texture_kind == 5, "a destroyed cubemap sent to a slot is named and not sent");
    }

    private static void ACubemapTheHostCannotStoreIsNoTextureAndIsNamed()
    {
        // Six faces of 5000 x 5000 RGBA32 texels hold 600 MB, above the 512 MiB the host stores.
        var huge = new Cubemap(5000, TextureFormat.RGBA32, false) { name = "Huge" };
        using var log = new LogRecorder();
        var material = NewMaterial("Test/Cube/Huge");
        var probe = new Texture2D(1, 1).RendererIdentity;
        material.SetTexture("_SkyCubemap", huge);
        Expect.That(ReadProperty(Material, material.WorldLink.Id, "_SkyCubemap").texture_kind == 5, "a cubemap beyond the host's limit is named and not sent");
        Expect.That(new Texture2D(1, 1).RendererIdentity == probe + 1, "it takes no identity");
        Expect.That(log.Messages.Any(m => m.Contains("'Huge'") && m.Contains("more than the host stores")), "the warning names the cubemap");
    }

    private static void ACubemapFromAPlayerCarriesItsPlanAndBytes()
    {
        // Six 2x2 RGBA32 faces, in sRGB, with a second level: 20 bytes a face.
        var cube = Cubemap.FromPlayer("Sky", new PlayerCubemap(2, 2, (int)TextureFormat.RGBA32, Linear: false, MipCount: 2, DataSize: 120));
        Expect.That(cube.name == "Sky" && cube.width == 2 && cube.height == 2 && cube.mipmapCount == 2, "the planned name, size and levels");
        Expect.That(cube.format == TextureFormat.RGBA32 && cube.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB, "the planned format in sRGB");
        Expect.Equal(120, cube.ByteLength, "the bytes of six faces of two levels");
        Expect.Throws<InvalidDataException>(() => Cubemap.FromPlayer("Short", new PlayerCubemap(2, 2, (int)TextureFormat.RGBA32, true, 2, 119)), "a plan whose size is not the bytes");
        Expect.Throws<InvalidDataException>(() => Cubemap.FromPlayer("Wide", new PlayerCubemap(2, 4, (int)TextureFormat.RGBA32, true, 1, 96)), "a plan whose faces are not square");
        Expect.Throws<InvalidDataException>(() => Cubemap.FromPlayer("Odd", new PlayerCubemap(2, 2, 9999, true, 1, 96)), "a plan with no such format");
    }
}
