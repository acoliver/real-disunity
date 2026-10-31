using System.Text.Json;
using UnityEngine;
using UnityEngine.U2D;

namespace Disunity.Hosting;

/// <summary>Building the textures, sprites and sprite atlases of a resource plan.</summary>
internal sealed partial class ResourceDatabase
{
    private Texture2D Texture(JsonElement asset)
    {
        var name = asset.Str("name");
        var data = asset.GetProperty("data");
        var plan = new PlayerTexture(asset.GetProperty("width").GetInt32(), asset.GetProperty("height").GetInt32(),
            asset.GetProperty("format").GetInt32(), asset.GetProperty("mip_count").GetInt32(),
            Linear: asset.GetProperty("color_space").GetInt32() == 0, Readable: asset.GetProperty("readable").GetBoolean(),
            DataSize: data.GetProperty("size").GetInt64());
        var texture = Texture2D.FromPlayer(name, plan);
        ReadBytes($"texture {name}", Path.Combine(_directory, data.Str("file")), data.GetProperty("offset").GetInt64(), texture.PixelBytes);
        ApplySampler(texture, asset);
        return texture;
    }

    private Texture2DArray TextureArray(JsonElement asset)
    {
        var name = asset.Str("name");
        var data = asset.GetProperty("data");
        var plan = new PlayerTextureArray(asset.GetProperty("width").GetInt32(), asset.GetProperty("height").GetInt32(), asset.GetProperty("depth").GetInt32(),
            asset.GetProperty("graphics_format").GetInt32(), asset.GetProperty("mip_count").GetInt32(), Readable: asset.GetProperty("readable").GetBoolean(),
            DataSize: data.GetProperty("size").GetInt64());
        var array = Texture2DArray.FromPlayer(name, plan);
        ReadBytes($"texture array {name}", Path.Combine(_directory, data.Str("file")), data.GetProperty("offset").GetInt64(), array.PixelBytes);
        ApplySampler(array, asset);
        return array;
    }

    /// <summary>The filter, wrap modes, anisotropy and mip bias a texture asset plans.</summary>
    private static void ApplySampler(Texture texture, JsonElement asset)
    {
        texture.filterMode = (FilterMode)asset.GetProperty("filter_mode").GetInt32();
        texture.wrapModeU = (TextureWrapMode)asset.GetProperty("wrap_u").GetInt32();
        texture.wrapModeV = (TextureWrapMode)asset.GetProperty("wrap_v").GetInt32();
        texture.wrapModeW = (TextureWrapMode)asset.GetProperty("wrap_w").GetInt32();
        texture.anisoLevel = asset.GetProperty("aniso_level").GetInt32();
        texture.mipMapBias = asset.GetProperty("mip_bias").GetSingle();
    }

    private Sprite PlayerSprite(JsonElement asset)
    {
        var name = asset.Str("name");
        var reference = asset.GetProperty("texture");
        if (reference.ValueKind != JsonValueKind.Object)
            throw new InvalidDataException($"resource sprite {name} has no texture");
        var texture = Asset(reference.GetProperty("$asset").GetInt32()) as Texture2D
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

    private SpriteAtlas Atlas(JsonElement asset)
    {
        var sprites = asset.GetProperty("sprites").EnumerateArray()
            .Select(reference => Asset(reference.GetProperty("$asset").GetInt32()) as Sprite
                ?? throw new InvalidDataException($"resource atlas {asset.Str("name")}: a packed sprite is not a Sprite"))
            .ToList();
        return new SpriteAtlas(asset.Str("name"), asset.Str("tag"), asset.GetProperty("is_variant").GetBoolean(), sprites);
    }

    private static float[] Floats(JsonElement asset, string field) =>
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
