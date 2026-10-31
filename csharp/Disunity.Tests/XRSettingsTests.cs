using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.XR;

/// <summary>XRSettings describes a machine with no XR device.</summary>
internal static class XRSettingsTests
{
    public static void Run()
    {
        NoDevice();
        Stored();
        LoadingDevices();
        Console.WriteLine("PASS: XRSettings reports no XR device, stores view settings and loads no device");
    }

    private static void NoDevice()
    {
        Expect.That(!XRSettings.enabled && !XRSettings.isDeviceActive, "XR is off");
        Expect.Equal(string.Empty, XRSettings.loadedDeviceName, "loadedDeviceName");
        Expect.Equal(0, XRSettings.supportedDevices.Length, "supportedDevices");
        Expect.Equal(0, XRSettings.eyeTextureWidth + XRSettings.eyeTextureHeight, "eye texture size");
        Expect.Equal(TextureDimension.None, XRSettings.deviceEyeTextureDimension, "deviceEyeTextureDimension");
        Expect.Equal(XRSettings.StereoRenderingMode.MultiPass, XRSettings.stereoRenderingMode, "stereoRenderingMode");
        Expect.Equal(0, XRSettings.eyeTextureDesc.width, "eyeTextureDesc");
        XRSettings.enabled = false;
        Expect.That(!XRSettings.enabled, "disabling stays disabled");
        var error = Expect.Catch<NotSupportedException>(() => XRSettings.enabled = true, "enable");
        Expect.That(error.Message.Contains("XRSettings.enabled") && error.Message.Contains("no XR device"), error.Message);
        Expect.That(!XRSettings.enabled, "a refused enable changes nothing");
    }

    private static void Stored()
    {
        Expect.Equal(1f, XRSettings.eyeTextureResolutionScale, "eyeTextureResolutionScale");
        Expect.Equal(1f, XRSettings.renderViewportScale, "renderViewportScale");
        Expect.Equal(1f, XRSettings.occlusionMaskScale, "occlusionMaskScale");
        Expect.That(XRSettings.useOcclusionMesh && XRSettings.showDeviceView, "mesh and device view default on");
        Expect.Equal(XRSettings.GameViewRenderMode.BothEyes, XRSettings.gameViewRenderMode, "gameViewRenderMode");
        XRSettings.eyeTextureResolutionScale = 1.5f;
        XRSettings.renderViewportScale = 0.5f;
        XRSettings.occlusionMaskScale = 2f;
        XRSettings.useOcclusionMesh = false;
        XRSettings.showDeviceView = false;
        XRSettings.gameViewRenderMode = XRSettings.GameViewRenderMode.LeftEye;
        Expect.Equal(1.5f, XRSettings.eyeTextureResolutionScale, "eyeTextureResolutionScale set");
        Expect.Equal(0.5f, XRSettings.renderViewportScale, "renderViewportScale set");
        Expect.Equal(2f, XRSettings.occlusionMaskScale, "occlusionMaskScale set");
        Expect.That(!XRSettings.useOcclusionMesh && !XRSettings.showDeviceView, "flags set");
        Expect.Equal(XRSettings.GameViewRenderMode.LeftEye, XRSettings.gameViewRenderMode, "gameViewRenderMode set");
        XRSettings.eyeTextureResolutionScale = 1f;
        XRSettings.renderViewportScale = 1f;
        XRSettings.occlusionMaskScale = 1f;
        XRSettings.useOcclusionMesh = true;
        XRSettings.showDeviceView = true;
        XRSettings.gameViewRenderMode = XRSettings.GameViewRenderMode.BothEyes;
    }

    private static void LoadingDevices()
    {
        XRSettings.LoadDeviceByName("");
        XRSettings.LoadDeviceByName("None");
        XRSettings.LoadDeviceByName(new[] { "None", "" });
        var named = Expect.Catch<NotSupportedException>(() => XRSettings.LoadDeviceByName("OpenVR"), "load a headset");
        Expect.That(named.Message.Contains("LoadDeviceByName(OpenVR)"), named.Message);
        Expect.Throws<NotSupportedException>(() => XRSettings.LoadDeviceByName(new[] { "None", "Oculus" }), "load from a list");
        Expect.Throws<ArgumentNullException>(() => XRSettings.LoadDeviceByName((string)null!), "null name");
        Expect.Throws<ArgumentNullException>(() => XRSettings.LoadDeviceByName((string[])null!), "null list");
    }
}
