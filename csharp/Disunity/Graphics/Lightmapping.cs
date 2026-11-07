using Unity.Collections;

namespace UnityEngine.Experimental.GlobalIllumination;

/// <summary>How a light takes part in global illumination.</summary>
public enum LightMode : byte
{
    Realtime = 0,
    Mixed = 1,
    Baked = 2,
    Unknown = 3,
}

/// <summary>A color in linear space with its intensity folded in, as a light hands it to global illumination.</summary>
public struct LinearColor
{
    public float red;
    public float green;
    public float blue;
    public float intensity;
}

/// <summary>One light as the global illumination system takes it. The engine bakes no lighting, so a light reaches
/// the system only through a delegate a render pipeline sets with <see cref="Lightmapping.SetDelegate"/>, which the
/// engine never calls.</summary>
public struct LightDataGI
{
    public int instanceID;
    public int cookieID;
    public float cookieScale;
    public LinearColor color;
    public LinearColor indirectColor;
    public Quaternion orientation;
    public Vector3 position;
    public float range;
    public float coneAngle;
    public float innerConeAngle;
    public float shape0;
    public float shape1;
    public LightType type;
    public LightMode mode;
    public byte shadow;
    public FalloffType falloff;
}

/// <summary>Global illumination baking. The engine bakes nothing, so a delegate set here is held and never called.</summary>
public static class Lightmapping
{
    /// <summary>Turns lights into the data global illumination reads; a render pipeline supplies one.</summary>
    public delegate void RequestLightsDelegate(Light[] requests, NativeArray<LightDataGI> lightsOutput);

    private static RequestLightsDelegate? _delegate;

    /// <summary>Sets the delegate that converts lights for global illumination; the engine only holds it.</summary>
    public static void SetDelegate(RequestLightsDelegate del) => Volatile.Write(ref _delegate, del);

    /// <summary>The delegate set, or null once reset.</summary>
    public static RequestLightsDelegate? GetDelegate() => Volatile.Read(ref _delegate);

    /// <summary>Clears the delegate.</summary>
    public static void ResetDelegate() => Volatile.Write(ref _delegate, null);
}
