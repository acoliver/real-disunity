using System.Collections;

namespace UnityEngine;

/// <summary>
/// Called around serialization. The engine never writes serialized data, so only
/// <see cref="OnAfterDeserialize"/> runs: after an object's fields are loaded from a scene or
/// asset, and for a serializable class or struct in a field, after its own fields.
/// </summary>
public interface ISerializationCallbackReceiver
{
    void OnBeforeSerialize();
    void OnAfterDeserialize();
}

/// <summary>A coroutine wait that lasts while <see cref="keepWaiting"/> is true, checked
/// once a frame.</summary>
public abstract class CustomYieldInstruction : IEnumerator
{
    public abstract bool keepWaiting { get; }
    public object? Current => null;
    public bool MoveNext() => keepWaiting;
    public virtual void Reset() { }
}

public enum ScreenOrientation
{
    Unknown = 0,
    Portrait = 1,
    PortraitUpsideDown = 2,
    LandscapeLeft = 3,
    Landscape = 3,
    LandscapeRight = 4,
    AutoRotation = 5,
}
