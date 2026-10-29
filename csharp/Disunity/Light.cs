using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A native 3D light. Every setting is read from and written to the light in the native world, so a
/// clone from <c>Instantiate</c> has its own settings. The engine draws no 3D lights yet, so nothing is
/// lit by it. The defaults are the engine's choices. The native side brings <see cref="spotAngle"/>,
/// <see cref="innerSpotAngle"/> and <see cref="shadowStrength"/> into range and rejects the rest of an
/// invalid write with a <see cref="DisunityException"/>, leaving every setting unchanged: a negative or
/// non-finite intensity, range, cookie size or bounce intensity, a color temperature that is not above
/// zero, a non-finite color or shadow bias, and a NaN angle or shadow strength. A light is a
/// <see cref="Behaviour"/> and not a <see cref="MonoBehaviour"/>, so a search for scripts does not find it.
/// </summary>
public sealed class Light : Behaviour
{
    private const string NotRendered = "the engine does not render 3D lights yet";

    private static readonly object CookieLock = new();
    private static readonly Dictionary<int, WeakReference<Texture>> Cookies = new();

    private unsafe DisunityLight Read()
    {
        DisunityLight value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_light_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    private unsafe void Write(DisunityLight value) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_light_write(DisunityHost.WorldHandle, Handle, &value));
    private void Update(Func<DisunityLight, DisunityLight> change) => Write(change(Read()));

    private protected override bool EnabledState { get => Read().enabled == 1; set => Update(l => { l.enabled = value ? 1u : 0u; return l; }); }
    public LightType type { get => (LightType)Read().kind; set => Update(l => { l.kind = (uint)value; return l; }); }
    public Color color
    {
        get { var l = Read(); return new Color(l.r, l.g, l.b, l.a); }
        set => Update(l => { (l.r, l.g, l.b, l.a) = (value.r, value.g, value.b, value.a); return l; });
    }
    public float intensity { get => Read().intensity; set => Update(l => { l.intensity = value; return l; }); }
    public float range { get => Read().range; set => Update(l => { l.range = value; return l; }); }

    /// <summary>The cone angle of a spot light in degrees, clamped to 1 through 179. A smaller angle lowers
    /// <see cref="innerSpotAngle"/> to match.</summary>
    public float spotAngle { get => Read().spot_angle; set => Update(l => { l.spot_angle = value; return l; }); }

    /// <summary>The inner cone angle in degrees, clamped to 0 through <see cref="spotAngle"/>.</summary>
    public float innerSpotAngle { get => Read().inner_spot_angle; set => Update(l => { l.inner_spot_angle = value; return l; }); }

    public LightShadows shadows { get => (LightShadows)Read().shadows; set => Update(l => { l.shadows = (uint)value; return l; }); }

    /// <summary>Clamped to 0 through 1.</summary>
    public float shadowStrength { get => Read().shadow_strength; set => Update(l => { l.shadow_strength = value; return l; }); }
    public float shadowBias { get => Read().shadow_bias; set => Update(l => { l.shadow_bias = value; return l; }); }
    public float shadowNormalBias { get => Read().shadow_normal_bias; set => Update(l => { l.shadow_normal_bias = value; return l; }); }
    public float shadowNearPlane { get => Read().shadow_near_plane; set => Update(l => { l.shadow_near_plane = value; return l; }); }
    public int cullingMask { get => unchecked((int)Read().culling_mask); set => Update(l => { l.culling_mask = unchecked((uint)value); return l; }); }
    public int renderingLayerMask { get => unchecked((int)Read().rendering_layer_mask); set => Update(l => { l.rendering_layer_mask = unchecked((uint)value); return l; }); }
    public float bounceIntensity { get => Read().bounce_intensity; set => Update(l => { l.bounce_intensity = value; return l; }); }

    /// <summary>In kelvin; finite and above zero.</summary>
    public float colorTemperature { get => Read().color_temperature; set => Update(l => { l.color_temperature = value; return l; }); }
    public bool useColorTemperature { get => Read().use_color_temperature == 1; set => Update(l => { l.use_color_temperature = value ? 1u : 0u; return l; }); }
    public float cookieSize { get => Read().cookie_size; set => Update(l => { l.cookie_size = value; return l; }); }
    public LightmapBakeType lightmapBakeType { get => (LightmapBakeType)Read().lightmap_bake_type; set => Update(l => { l.lightmap_bake_type = (uint)value; return l; }); }

    /// <summary>The cookie texture, or null. It is stored by instance id, so a clone shares it.</summary>
    public Texture? cookie
    {
        get
        {
            var id = Read().cookie_texture;
            if (id == 0) return null;
            lock (CookieLock)
            {
                if (!Cookies.TryGetValue(unchecked((int)(uint)id), out var weak) || !weak.TryGetTarget(out var texture)) return null;
                return texture ? texture : null;
            }
        }
        set
        {
            if (value is not null && !value) throw new ArgumentException("the cookie texture was destroyed", nameof(value));
            var id = value is null ? 0ul : unchecked((ulong)(uint)value.GetInstanceID());
            if (value is not null) lock (CookieLock) Cookies[value.GetInstanceID()] = new WeakReference<Texture>(value);
            Update(l => { l.cookie_texture = id; return l; });
        }
    }

    public void AddCommandBuffer(LightEvent evt, CommandBuffer buffer) =>
        throw new NotSupportedException($"Light.AddCommandBuffer: {NotRendered}");

    public void RemoveCommandBuffer(LightEvent evt, CommandBuffer buffer) =>
        throw new NotSupportedException($"Light.RemoveCommandBuffer: {NotRendered}");
}
