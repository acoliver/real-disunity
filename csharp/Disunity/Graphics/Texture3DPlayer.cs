using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>What a player's resource plan says about a 3D texture before its pixels are read.</summary>
internal readonly record struct PlayerTexture3D(int Width, int Height, int Depth, int GraphicsFormat, int MipCount, bool Readable, long DataSize);

public sealed partial class Texture3D
{
    /// <summary>The raw bytes of every level, which a player's pixels are read into.</summary>
    internal Span<byte> PixelBytes => _data;

    /// <summary>The size in bytes of the whole mip chain, each level with its slices.</summary>
    internal int ByteLength => _data.Length;

    /// <summary>The identity the renderer knows this texture by: allocated the first time a material or a property block
    /// sends it, from <see cref="DisunityHost.FirstPlayerTextureIdentity"/> up, and kept for the texture's lifetime.</summary>
    internal ulong RendererIdentity => TakeRendererIdentity();

    /// <summary>Hands the texture's raw bytes and description to the host: its depth, its dimension (3D), its
    /// <see cref="GraphicsFormat"/> and its <see cref="TextureFormat"/>, with the bytes of each mip level one after another and
    /// the slices of a level one after another. Called when the texture takes its identity and on
    /// <see cref="Apply(bool, bool)"/>.</summary>
    internal override void StoreForRenderer() => StoreRecord(new DisunityTextureInfo
    {
        format = _format is { } known ? (uint)known : 0u, width = (uint)_width, height = (uint)_height, mips = (uint)_mipCount,
        linear = GraphicsFormats.IsSrgb(_graphics) ? 0u : 1u, depth = (uint)_depth, graphics_format = (uint)_graphics,
        dimension = (uint)TextureDimension.Tex3D,
    }, _data);

    /// <summary>Makes a zeroed texture of the planned size, format and mip count, whose raw size must equal
    /// <see cref="PlayerTexture3D.DataSize"/>. A format the engine cannot lay out keeps the planned size as raw bytes.
    /// Failures name the texture.</summary>
    internal static Texture3D FromPlayer(string name, PlayerTexture3D plan)
    {
        var graphics = (GraphicsFormat)plan.GraphicsFormat;
        var format = GraphicsFormats.LayoutOf(graphics);
        Texture3D texture;
        try { texture = new Texture3D(plan.Width, plan.Height, plan.Depth, format, graphics, plan.MipCount, plan.DataSize); }
        catch (ArgumentException error) { throw new InvalidDataException($"resource texture {name}: {error.Message}", error); }
        catch (NotSupportedException error) { throw new NotSupportedException($"resource texture {name}: {error.Message}", error); }
        if (texture._data.Length != plan.DataSize)
            throw new InvalidDataException($"resource texture {name}: the plan holds {plan.DataSize} bytes, but a {plan.Width}x{plan.Height}x{plan.Depth} {graphics} texture with {plan.MipCount} mip levels needs {texture._data.Length}");
        texture.name = name;
        texture._readable = plan.Readable;
        return texture;
    }
}
