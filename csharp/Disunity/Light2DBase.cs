namespace UnityEngine.U2D;

/// <summary>
/// The base of a 2D light script. The engine's own <c>Light2D</c> is a separate type; a render pipeline package
/// derives its light from this one, so the type exists for that package's types to load.
/// </summary>
public abstract class Light2DBase : MonoBehaviour
{
}
