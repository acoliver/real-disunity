using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Experimental.Rendering;

namespace UnityEngine;

/// <summary>What a player's resource plan says about a 2D texture before its pixels are read.</summary>
internal readonly record struct PlayerTexture(int Width, int Height, int Format, int MipCount, bool Linear, bool Readable, long DataSize);

public sealed partial class Texture2D
{
    private ulong _identity;

    /// <summary>Zero for colour, data and coverage. After the font engine draws distance fields into an Alpha8 atlas, the
    /// spread in texels they were drawn with, which the renderer needs to turn the field back into coverage.</summary>
    internal uint DistanceSpread { get; set; }

    /// <summary>The identity the renderer knows this texture by: allocated the first time a sprite is made over it,
    /// from <see cref="DisunityHost.FirstPlayerTextureIdentity"/> up, and kept for the texture's lifetime.</summary>
    internal ulong RendererIdentity
    {
        get
        {
            if (_identity != 0) return _identity;
            _identity = DisunityHost.AllocatePlayerTextureIdentity(this);
            StoreForRenderer();
            return _identity;
        }
    }

    /// <summary>Hands the texture's raw bytes and description to the host, which draws from them: called when the
    /// texture takes its identity and on <see cref="Apply(bool, bool)"/>. A texture of no size has nothing to draw.</summary>
    private unsafe void StoreForRenderer()
    {
        if (_width == 0 || _height == 0) return;
        DisunityHost.EnsureLoaded();
        var info = new DisunityTextureInfo
        {
            identity = _identity, format = (uint)_format, width = (uint)_width, height = (uint)_height, mips = (uint)_mipCount,
            linear = GraphicsFormats.IsSrgb(_graphics) ? 0u : 1u, length = (uint)_data.Length, distance_spread = DistanceSpread,
        };
        fixed (byte* bytes = _data)
            DisunityHost.ThrowIfError(NativeBindings.disunity_texture_store(DisunityHost.WorldHandle, _identity, &info, bytes, info.length));
    }

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
