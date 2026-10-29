using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine.Rendering.Universal;

public sealed class Light2D : MonoBehaviour
{
    private protected override bool EnabledState { get => NativeBehaviourState.Read(Handle); set => NativeBehaviourState.Write(Handle, value); }
    public enum LightType { Parametric = 0, Freeform = 1, Sprite = 2, Point = 3, Global = 4, Spot = 5 }
    public LightType lightType
    {
        get => LightType.Global;
        set { if (value != LightType.Global) throw new UnsupportedLight2DException("lightType"); }
    }
    public int blendStyleIndex
    {
        get => 0;
        set { if (value != 0) throw new UnsupportedLight2DException("blendStyleIndex"); }
    }
    private unsafe float Read(uint property)
    {
        float value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_light2d_read(DisunityHost.WorldHandle, Handle, property, &value));
        return value;
    }
    private void Write(uint property, float value) => DisunityHost.ThrowIfError(NativeBindings.disunity_light2d_write(DisunityHost.WorldHandle, Handle, property, value));
    public unsafe int[] affectedSortingLayers
    {
        get
        {
            uint count;
            DisunityHost.ThrowIfError(NativeBindings.disunity_light2d_layers_read(DisunityHost.WorldHandle, Handle, null, 0, &count));
            var layers = new int[count];
            fixed (int* pointer = layers)
                DisunityHost.ThrowIfError(NativeBindings.disunity_light2d_layers_read(DisunityHost.WorldHandle, Handle, pointer, count, &count));
            return layers;
        }
        set
        {
            ArgumentNullException.ThrowIfNull(value);
            fixed (int* pointer = value)
                DisunityHost.ThrowIfError(NativeBindings.disunity_light2d_layers(DisunityHost.WorldHandle, Handle, pointer, (uint)value.Length));
        }
    }
    public float intensity { get => Read(4); set => Write(4, value); }
    public Color color
    {
        get => new(Read(0), Read(1), Read(2), Read(3));
        set => DisunityHost.ThrowIfError(NativeBindings.disunity_light2d_color(DisunityHost.WorldHandle, Handle, value.r, value.g, value.b, value.a));
    }
}

public sealed class UnsupportedLight2DException(string field) : NotSupportedException($"Unsupported Light2D field: {field}") { public string Field { get; } = field; }
public sealed class ShadowCaster2D : MonoBehaviour
{
    public ShadowCaster2D() => throw new UnsupportedLight2DException("ShadowCaster2D");
}
