using Disunity.Hosting;
using Disunity.Native;
namespace UnityEngine;
internal static class NativeBehaviourState
{
    internal static bool IsNative(Component value) => value is AudioListener or GUILayer or FlareLayer or Animator or Rendering.SortingGroup or Rendering.Universal.Light2D or Rendering.Universal.UniversalAdditionalCameraData;
    internal static unsafe bool Read(ulong handle)
    {
        uint value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_camera_layer_read(DisunityHost.WorldHandle, handle, &value));
        return value == 1;
    }
    internal static void Write(ulong handle, bool value) => DisunityHost.ThrowIfError(NativeBindings.disunity_camera_layer_write(DisunityHost.WorldHandle, handle, value ? 1u : 0u));
}
public sealed class AudioListener : Behaviour
{
    private protected override bool EnabledState { get => NativeBehaviourState.Read(Handle); set => NativeBehaviourState.Write(Handle, value); }
    private static float _volume = 1;
    public static float volume { get => _volume; set => _volume = Math.Clamp(value, 0, 1); }
    public static bool pause { get; set; }
}
public sealed class GUILayer : Behaviour
{
    private protected override bool EnabledState { get => NativeBehaviourState.Read(Handle); set => NativeBehaviourState.Write(Handle, value); }
}
public sealed class FlareLayer : Behaviour
{
    private protected override bool EnabledState { get => NativeBehaviourState.Read(Handle); set => NativeBehaviourState.Write(Handle, value); }
}
