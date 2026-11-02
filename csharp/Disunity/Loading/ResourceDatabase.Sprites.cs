using System.Text.Json;
using UnityEngine;
using UnityEngine.U2D;

namespace Disunity.Hosting;

/// <summary>Building the textures, sprites and sprite atlases of a resource plan.</summary>
internal sealed partial class ResourceDatabase
{
    private Texture2D Texture(JsonElement asset) => PlayerSprites.BuildTexture(asset, _directory);

    private Sprite PlayerSprite(JsonElement asset) => PlayerSprites.BuildSprite(asset, Asset);

    private Texture2DArray TextureArray(JsonElement asset)
    {
        var name = asset.Str("name");
        var data = asset.GetProperty("data");
        var plan = new PlayerTextureArray(asset.GetProperty("width").GetInt32(), asset.GetProperty("height").GetInt32(), asset.GetProperty("depth").GetInt32(),
            asset.GetProperty("graphics_format").GetInt32(), asset.GetProperty("mip_count").GetInt32(), Readable: asset.GetProperty("readable").GetBoolean(),
            DataSize: data.GetProperty("size").GetInt64());
        var array = Texture2DArray.FromPlayer(name, plan);
        ReadBytes($"texture array {name}", Path.Combine(_directory, data.Str("file")), data.GetProperty("offset").GetInt64(), array.PixelBytes);
        PlayerSprites.ApplySampler(array, asset);
        return array;
    }

    private SpriteAtlas Atlas(JsonElement asset)
    {
        var sprites = asset.GetProperty("sprites").EnumerateArray()
            .Select(reference => Asset(reference.GetProperty("$asset").GetInt32()) as Sprite
                ?? throw new InvalidDataException($"resource atlas {asset.Str("name")}: a packed sprite is not a Sprite"))
            .ToList();
        return new SpriteAtlas(asset.Str("name"), asset.Str("tag"), asset.GetProperty("is_variant").GetBoolean(), sprites);
    }
}
