using System.Text.Json;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace Disunity.Hosting;

/// <summary>Building the render textures of a resource plan. The engine has no GPU surface, so a built texture holds
/// the planned description and sampler and nothing draws into it.</summary>
internal sealed partial class ResourceDatabase
{
    private static RenderTexture PlayerRenderTexture(JsonElement asset)
    {
        var descriptor = new RenderTextureDescriptor(asset.GetProperty("width").GetInt32(), asset.GetProperty("height").GetInt32(),
            (GraphicsFormat)asset.GetProperty("color_format").GetInt32(), (GraphicsFormat)asset.GetProperty("depth_format").GetInt32())
        {
            msaaSamples = asset.GetProperty("anti_aliasing").GetInt32(),
            useMipMap = asset.GetProperty("mip_maps").GetBoolean(),
            autoGenerateMips = asset.GetProperty("generate_mips").GetBoolean(),
            dimension = (TextureDimension)asset.GetProperty("dimension").GetInt32(),
        };
        return new RenderTexture(descriptor)
        {
            name = asset.Str("name"),
            filterMode = (FilterMode)asset.GetProperty("filter_mode").GetInt32(),
            wrapModeU = (TextureWrapMode)asset.GetProperty("wrap_u").GetInt32(),
            wrapModeV = (TextureWrapMode)asset.GetProperty("wrap_v").GetInt32(),
            wrapModeW = (TextureWrapMode)asset.GetProperty("wrap_w").GetInt32(),
            anisoLevel = asset.GetProperty("aniso_level").GetInt32(),
        };
    }
}
