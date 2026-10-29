using Disunity.Hosting;
using UnityEngine.Experimental.Rendering;

namespace UnityEngine;

/// <summary>What a player's resource plan says about a 2D texture before its pixels are read.</summary>
internal readonly record struct PlayerTexture(int Width, int Height, int Format, int MipCount, bool Linear, bool Readable, long DataSize);

public sealed partial class Texture2D
{
    private ulong _identity;

    /// <summary>The identity the renderer knows this texture by: allocated the first time a sprite is made over it,
    /// from <see cref="DisunityHost.FirstPlayerTextureIdentity"/> up, and kept for the texture's lifetime.</summary>
    internal ulong RendererIdentity => _identity != 0 ? _identity : _identity = DisunityHost.AllocatePlayerTextureIdentity(this);

    /// <summary>The raw bytes, which a player's pixels are read into.</summary>
    internal Span<byte> PixelBytes => _data;

    /// <summary>Makes a zeroed texture of the planned size, format and mip count, whose raw size must equal
    /// <see cref="PlayerTexture.DataSize"/>. Failures name the texture.</summary>
    internal static Texture2D FromPlayer(string name, PlayerTexture plan)
    {
        if (!Enum.IsDefined(typeof(TextureFormat), plan.Format))
            throw new InvalidDataException($"resource texture {name}: format {plan.Format} is not a TextureFormat");
        var format = (TextureFormat)plan.Format;
        Texture2D texture;
        try
        {
            texture = new Texture2D(plan.Width, plan.Height, format, GraphicsFormats.OfTexture(format, plan.Linear), plan.MipCount, null, false);
        }
        catch (ArgumentException error) { throw new InvalidDataException($"resource texture {name}: {error.Message}", error); }
        catch (NotSupportedException error) { throw new NotSupportedException($"resource texture {name}: {error.Message}", error); }
        if (texture._data.Length != plan.DataSize)
            throw new InvalidDataException($"resource texture {name}: the plan holds {plan.DataSize} bytes, but a {plan.Width}x{plan.Height} {format} texture with {plan.MipCount} mip levels needs {texture._data.Length}");
        texture.name = name;
        texture._readable = plan.Readable;
        return texture;
    }
}
