using Disunity.Hosting;
using Disunity.Native;
namespace UnityEngine.Rendering.Universal;
public enum CameraOverrideOption { Off = 0, On = 1, UsePipelineSettings = 2 }
public enum CameraRenderType { Base, Overlay }
public enum AntialiasingMode { None, FastApproximateAntialiasing, SubpixelMorphologicalAntiAliasing, TemporalAntiAliasing }
public enum AntialiasingQuality { Low, Medium, High }
public sealed class UnsupportedCameraSettingException(string field) : NotSupportedException($"Unsupported UniversalAdditionalCameraData field: {field}")
{ public string Field { get; } = field; }
public sealed class UniversalAdditionalCameraData : MonoBehaviour
{
    public override bool enabled { get => NativeBehaviourState.Read(Handle); set => NativeBehaviourState.Write(Handle, value); }
    private unsafe uint Read(uint property)
    {
        uint value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_camera_data_read(DisunityHost.WorldHandle, Handle, property, &value));
        return value;
    }
    private void Write(uint property, uint value, string field, bool supported)
    {
        if (!supported) throw new UnsupportedCameraSettingException(field);
        DisunityHost.ThrowIfError(NativeBindings.disunity_camera_data_write(DisunityHost.WorldHandle, Handle, property, value));
    }
    public bool renderPostProcessing { get => Read(0) != 0; set => Write(0, value ? 1u : 0u, nameof(renderPostProcessing), !value); }
    public AntialiasingMode antialiasing { get => (AntialiasingMode)Read(1); set => Write(1, (uint)value, nameof(antialiasing), value == AntialiasingMode.None); }
    public bool requiresDepthTexture { get => Read(2) != 0; set => Write(2, value ? 1u : 0u, nameof(requiresDepthTexture), !value); }
    public bool requiresColorTexture { get => Read(3) != 0; set => Write(3, value ? 1u : 0u, nameof(requiresColorTexture), !value); }
    public CameraRenderType renderType { get => (CameraRenderType)Read(4); set => Write(4, (uint)value, nameof(renderType), value == CameraRenderType.Base); }
    public bool renderShadows { get => Read(5) != 0; set => Write(5, value ? 1u : 0u, nameof(renderShadows), !value); }
    public bool stopNaN { get => Read(6) != 0; set => Write(6, value ? 1u : 0u, nameof(stopNaN), !value); }
    public bool dithering { get => Read(7) != 0; set => Write(7, value ? 1u : 0u, nameof(dithering), !value); }
    public CameraOverrideOption requiresDepthOption { get => (CameraOverrideOption)Read(8); set => Write(8, (uint)value, nameof(requiresDepthOption), value is CameraOverrideOption.Off or CameraOverrideOption.UsePipelineSettings); }
    public CameraOverrideOption requiresColorOption { get => (CameraOverrideOption)Read(9); set => Write(9, (uint)value, nameof(requiresColorOption), value is CameraOverrideOption.Off or CameraOverrideOption.UsePipelineSettings); }
    public LayerMask volumeLayerMask { get => unchecked((int)Read(10)); set => Write(10, unchecked((uint)value.value), nameof(volumeLayerMask), true); }
    public bool allowXRRendering { get => Read(11) != 0; set => Write(11, value ? 1u : 0u, nameof(allowXRRendering), true); }
    public void SetRenderer(int index) { if (index != -1 && index != 0) throw new UnsupportedCameraSettingException("rendererIndex"); }
}
