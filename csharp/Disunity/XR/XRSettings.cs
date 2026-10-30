using UnityEngine.Rendering;

namespace UnityEngine.XR;

/// <summary>
/// Global XR state. The engine has no XR device, so XR is never enabled, no device is active or loaded, and
/// no device is supported. The scale and view settings are stored and change nothing.
/// </summary>
public static class XRSettings
{
    public enum StereoRenderingMode
    {
        MultiPass = 0,
        SinglePass = 1,
        SinglePassInstanced = 2,
        SinglePassMultiview = 3,
    }

    public enum GameViewRenderMode
    {
        None = 0,
        LeftEye = 1,
        RightEye = 2,
        BothEyes = 3,
        OcclusionMesh = 4,
    }

    private static readonly object Gate = new();
    private static float _eyeTextureResolutionScale = 1f;
    private static float _renderViewportScale = 1f;
    private static float _occlusionMaskScale = 1f;
    private static bool _useOcclusionMesh = true;
    private static bool _showDeviceView = true;
    private static GameViewRenderMode _gameViewRenderMode = GameViewRenderMode.BothEyes;

    /// <summary>Always false. Setting it to true throws <see cref="NotSupportedException"/>.</summary>
    public static bool enabled
    {
        get => false;
        set
        {
            if (value) throw NoXrDevice.Member($"{nameof(XRSettings)}.{nameof(enabled)}");
        }
    }

    public static bool isDeviceActive => false;

    public static string loadedDeviceName => string.Empty;

    public static string[] supportedDevices => Array.Empty<string>();

    public static int eyeTextureWidth => 0;

    public static int eyeTextureHeight => 0;

    public static RenderTextureDescriptor eyeTextureDesc => default;

    public static TextureDimension deviceEyeTextureDimension => TextureDimension.None;

    public static StereoRenderingMode stereoRenderingMode => StereoRenderingMode.MultiPass;

    public static float eyeTextureResolutionScale
    {
        get { lock (Gate) return _eyeTextureResolutionScale; }
        set { lock (Gate) _eyeTextureResolutionScale = value; }
    }

    public static float renderViewportScale
    {
        get { lock (Gate) return _renderViewportScale; }
        set { lock (Gate) _renderViewportScale = value; }
    }

    public static float occlusionMaskScale
    {
        get { lock (Gate) return _occlusionMaskScale; }
        set { lock (Gate) _occlusionMaskScale = value; }
    }

    public static bool useOcclusionMesh
    {
        get { lock (Gate) return _useOcclusionMesh; }
        set { lock (Gate) _useOcclusionMesh = value; }
    }

    public static bool showDeviceView
    {
        get { lock (Gate) return _showDeviceView; }
        set { lock (Gate) _showDeviceView = value; }
    }

    public static GameViewRenderMode gameViewRenderMode
    {
        get { lock (Gate) return _gameViewRenderMode; }
        set { lock (Gate) _gameViewRenderMode = value; }
    }

    /// <summary>Only "" and "None" load, because they ask for no device. Any other name throws.</summary>
    public static void LoadDeviceByName(string deviceName)
    {
        ArgumentNullException.ThrowIfNull(deviceName);
        if (deviceName.Length != 0 && deviceName != "None")
            throw NoXrDevice.Member($"{nameof(XRSettings)}.{nameof(LoadDeviceByName)}({deviceName})");
    }

    /// <summary>Only a list that names no device other than "" and "None" loads.</summary>
    public static void LoadDeviceByName(string[] prioritizedDeviceNameList)
    {
        ArgumentNullException.ThrowIfNull(prioritizedDeviceNameList);
        foreach (var name in prioritizedDeviceNameList) LoadDeviceByName(name);
    }
}
