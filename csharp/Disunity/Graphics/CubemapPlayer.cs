using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>What a player's resource plan says about a cubemap before its pixels are read: the size of a face, which must be
/// square, its <see cref="TextureFormat"/> and colour space, its mip count and the size of the bytes of the six faces.</summary>
internal readonly record struct PlayerCubemap(int Width, int Height, int Format, bool Linear, int MipCount, long DataSize);

public partial class Cubemap
{
    /// <summary>The identity the renderer knows this cubemap by: allocated the first time a material or a property block
    /// sends it, from <see cref="DisunityHost.FirstPlayerTextureIdentity"/> up, and kept for the cubemap's lifetime.</summary>
    internal ulong RendererIdentity => TakeRendererIdentity();

    /// <summary>Hands the cubemap's raw bytes and description to the host: six faces, the cube dimension, its
    /// <see cref="GraphicsFormat"/> and its <see cref="TextureFormat"/>. Called when the cubemap takes its identity.</summary>
    internal override void StoreForRenderer() => StoreRecord(new DisunityTextureInfo
    {
        format = _format is { } known ? (uint)known : 0u, width = (uint)_size, height = (uint)_size, mips = (uint)_mipCount,
        linear = GraphicsFormats.IsSrgb(_graphics) ? 0u : 1u, depth = Faces, graphics_format = (uint)_graphics,
        dimension = (uint)TextureDimension.Cube,
    }, _data);

    /// <summary>Makes a zeroed cubemap of the planned size, format and mip count, whose raw size must equal
    /// <see cref="PlayerCubemap.DataSize"/>. A format the engine cannot lay out keeps the planned size as raw bytes.
    /// Failures name the cubemap.</summary>
    internal static Cubemap FromPlayer(string name, PlayerCubemap plan)
    {
        if (plan.Width != plan.Height)
            throw new InvalidDataException($"resource cubemap {name}: its faces are {plan.Width}x{plan.Height}, and the faces of a cube are square");
        if (!Enum.IsDefined(typeof(TextureFormat), plan.Format))
            throw new InvalidDataException($"resource cubemap {name}: format {plan.Format} is not a TextureFormat");
        var format = (TextureFormat)plan.Format;
        Cubemap cube;
        try { cube = new Cubemap(plan.Width, format, GraphicsFormats.OfTexture(format, plan.Linear), plan.MipCount, plan.DataSize); }
        catch (ArgumentException error) { throw new InvalidDataException($"resource cubemap {name}: {error.Message}", error); }
        catch (NotSupportedException error) { throw new NotSupportedException($"resource cubemap {name}: {error.Message}", error); }
        if (cube._data.Length != plan.DataSize)
            throw new InvalidDataException($"resource cubemap {name}: the plan holds {plan.DataSize} bytes, but a {plan.Width} {format} cubemap with {plan.MipCount} mip levels needs {cube._data.Length}");
        cube.name = name;
        return cube;
    }
}
