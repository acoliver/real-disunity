namespace UnityEngine.Rendering;

/// <summary>The rendering features the active render pipeline supports. A pipeline replaces <see cref="active"/> when it
/// is created; the engine only holds the value.</summary>
public class SupportedRenderingFeatures
{
    private static SupportedRenderingFeatures _active = new();

    /// <summary>The features of the active pipeline; a new set of defaults until a pipeline replaces it.</summary>
    public static SupportedRenderingFeatures active
    {
        get => Volatile.Read(ref _active);
        set => Volatile.Write(ref _active, value ?? throw new ArgumentNullException(nameof(value)));
    }
}
