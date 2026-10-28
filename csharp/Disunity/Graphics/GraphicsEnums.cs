namespace UnityEngine
{
    public enum AnisotropicFiltering { Disable = 0, Enable = 1, ForceEnable = 2 }

    [Flags]
    public enum MaterialGlobalIlluminationFlags { None = 0, RealtimeEmissive = 1, BakedEmissive = 2, EmissiveIsBlack = 4, AnyEmissive = 3 }

    public enum MaterialPropertyType { Float = 0, Int = 1, Vector = 2, Matrix = 3, Texture = 4, ConstantBuffer = 5, ComputeBuffer = 6 }

    [Flags]
    public enum RenderTextureCreationFlags
    {
        MipMap = 1, AutoGenerateMips = 2, SRGB = 4, EyeTexture = 8, EnableRandomWrite = 16, CreatedFromScript = 32,
        AllowVerticalFlip = 128, NoResolvedColorSurface = 256, DynamicallyScalable = 1024, BindMS = 2048,
        ShadingRate = 16384, DynamicallyScalableExplicit = 131072,
    }
}

namespace UnityEngine.Rendering
{
    public enum ShaderHardwareTier { Tier1 = 0, Tier2 = 1, Tier3 = 2 }

    public enum ShaderPropertyType { Color = 0, Vector = 1, Float = 2, Range = 3, Texture = 4, Int = 5 }

    [Flags]
    public enum ShaderPropertyFlags
    {
        None = 0, HideInInspector = 1, PerRendererData = 2, NoScaleOffset = 4, Normal = 8, HDR = 16, Gamma = 32,
        NonModifiableTextureData = 64, MainTexture = 128, MainColor = 256, Vector2 = 512, Vector3 = 1024,
    }

    public enum ShaderKeywordType { None = 0, BuiltinDefault = 2, BuiltinExtra = 6, BuiltinAutoStripped = 10, UserDefined = 16, Plugin = 32 }
}
