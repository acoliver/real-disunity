using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Building a texture and a sprite of a plan, for resource plans and scene plans alike: <c>{"type":
/// "Texture2D", "name", "width", "height", "format", "mip_count", "readable", "color_space", "filter_mode", "wrap_u",
/// "wrap_v", "wrap_w", "aniso_level", "mip_bias", "data": {"file", "offset", "size"}}</c> and <c>{"type": "Sprite",
/// "name", "rect", "pivot", "pixels_per_unit", "border", "texture" ({"$asset": i}), "texture_rect",
/// "texture_rect_offset", "packed"}</c>. A texture's <c>file</c> is relative to the plan's directory, or absolute.</summary>
internal static class PlayerSprites
{
    /// <summary>Builds the texture, reading its pixels from <c>data.file</c> under <paramref name="directory"/>.</summary>
    public static Texture2D BuildTexture(JsonElement asset, string directory)
    {
        var name = asset.Str("name");
        var data = asset.GetProperty("data");
        var plan = new PlayerTexture(asset.GetProperty("width").GetInt32(), asset.GetProperty("height").GetInt32(),
            asset.GetProperty("format").GetInt32(), asset.GetProperty("mip_count").GetInt32(),
            Linear: asset.GetProperty("color_space").GetInt32() == 0, Readable: asset.GetProperty("readable").GetBoolean(),
            DataSize: data.GetProperty("size").GetInt64());
        var texture = Texture2D.FromPlayer(name, plan);
        ResourceDatabase.ReadBytes($"texture {name}", Path.Combine(directory, data.Str("file")), data.GetProperty("offset").GetInt64(), texture.PixelBytes);
        ApplySampler(texture, asset);
        return texture;
    }

    /// <summary>The filter, wrap modes, anisotropy and mip bias a texture asset plans.</summary>
    public static void ApplySampler(Texture texture, JsonElement asset)
    {
        texture.filterMode = (FilterMode)asset.GetProperty("filter_mode").GetInt32();
        texture.wrapModeU = (TextureWrapMode)asset.GetProperty("wrap_u").GetInt32();
        texture.wrapModeV = (TextureWrapMode)asset.GetProperty("wrap_v").GetInt32();
        texture.wrapModeW = (TextureWrapMode)asset.GetProperty("wrap_w").GetInt32();
        texture.anisoLevel = asset.GetProperty("aniso_level").GetInt32();
        texture.mipMapBias = asset.GetProperty("mip_bias").GetSingle();
    }

    /// <summary>Builds the sprite over the texture <paramref name="resolve"/> gives for its planned asset index.</summary>
    public static Sprite BuildSprite(JsonElement asset, Func<int, object> resolve)
    {
        var name = asset.Str("name");
        var reference = asset.GetProperty("texture");
        if (reference.ValueKind != JsonValueKind.Object)
            throw new InvalidDataException($"resource sprite {name} has no texture");
        var texture = resolve(reference.GetProperty("$asset").GetInt32()) as Texture2D
            ?? throw new InvalidDataException($"resource sprite {name}: its texture is not a Texture2D");
        var details = new SpriteDetails(ReadRect(asset, "rect"), ReadRect(asset, "texture_rect"),
            ReadVector2(asset, "texture_rect_offset"), ReadVector4(asset, "border"), asset.GetProperty("packed").GetBoolean());
        try
        {
            var sprite = DisunityHost.CreateSprite(texture, details, ReadVector2(asset, "pivot"), asset.GetProperty("pixels_per_unit").GetSingle());
            DisunityHost.NameSprite(sprite, name);
            return sprite;
        }
        catch (DisunityException error)
        {
            throw new InvalidDataException($"resource sprite {name}: {error.Message}", error);
        }
    }

    internal static float[] Floats(JsonElement asset, string field) =>
        asset.GetProperty(field).EnumerateArray().Select(value => value.GetSingle()).ToArray();

    private static Rect ReadRect(JsonElement asset, string field) => Floats(asset, field) is [var x, var y, var width, var height]
        ? new Rect(x, y, width, height)
        : throw new InvalidDataException($"resource {asset.Str("name")}: {field} is not four numbers");

    private static Vector2 ReadVector2(JsonElement asset, string field) => Floats(asset, field) is [var x, var y]
        ? new Vector2(x, y)
        : throw new InvalidDataException($"resource {asset.Str("name")}: {field} is not two numbers");

    private static Vector4 ReadVector4(JsonElement asset, string field) => Floats(asset, field) is [var x, var y, var z, var w]
        ? new Vector4(x, y, z, w)
        : throw new InvalidDataException($"resource {asset.Str("name")}: {field} is not four numbers");
}
