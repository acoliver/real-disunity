namespace Disunity;

/// <summary>
/// Script lifecycle over the native world. The host drives the phases:
/// Awake on load/create, Start before the first Update, Update on each
/// simulation tick, LateUpdate after Update before commands are applied,
/// OnDestroy when the world unloads.
/// </summary>
public abstract class MonoBehaviour
{
    protected MonoBehaviour()
    {
    }

    public GameObject gameObject { get; internal set; } = null!;

    protected internal virtual void Awake() { }
    protected internal virtual void Start() { }
    protected internal virtual void Update() { }
    protected internal virtual void LateUpdate() { }
    protected internal virtual void OnDestroy() { }
}
