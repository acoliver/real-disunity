using UnityEngine;
using UnityEngine.Assertions;
using UnityEngine.Rendering;
using UnityEngine.SceneManagement;

/// <summary>
/// Spot checks of the enums generated from a Unity 6000.3 player's module metadata against known
/// values, and the message of a failed assertion.
/// </summary>
internal static class EngineEnumTests
{
    public static void Run()
    {
        Require((int)TextureFormat.RGBA32 == 4 && (int)TextureFormat.ARGB32 == 5, "TextureFormat");
        Require((int)RenderTextureFormat.ARGB32 == 0 && (int)RenderTextureFormat.Depth == 1, "RenderTextureFormat");
        Require((DepthTextureMode.Depth | DepthTextureMode.MotionVectors) == (DepthTextureMode)5
            && typeof(DepthTextureMode).IsDefined(typeof(FlagsAttribute), false), "DepthTextureMode is a flags enum");
        Require((int)HideFlags.DontSave == 52 && (int)HideFlags.HideAndDontSave == 61, "HideFlags");
        Require((int)CameraType.Game == 1 && (int)CameraType.SceneView == 2, "CameraType");
        Require((int)FilterMode.Point == 0 && (int)FilterMode.Trilinear == 2, "FilterMode");
        Require((int)BlendMode.SrcAlpha == 5 && (int)BlendMode.OneMinusSrcAlpha == 10, "BlendMode");
        Require((int)CompareFunction.LessEqual == 4 && (int)CompareFunction.Always == 8, "CompareFunction");
        Require((int)CullMode.Back == 2, "CullMode");
        Require((int)GraphicsDeviceType.Direct3D11 == 2 && (int)GraphicsDeviceType.Vulkan == 21, "GraphicsDeviceType");
        Require((int)LoadSceneMode.Additive == 1, "LoadSceneMode");
        Require(new AssertionException("Value was null.", "camera").Message == "Value was null.\ncamera"
            && new AssertionException("Value was null.", null).Message == "Value was null.", "an assertion's message adds the caller's");
        Console.WriteLine("PASS: generated engine enums match Unity's values");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException($"engine enums: {message}");
    }
}
