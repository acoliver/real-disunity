namespace UnityEngine;

/// <summary>Script methods are bound by name; native state owns message eligibility.</summary>
public abstract class MonoBehaviour : Behaviour
{
    private protected override unsafe bool EnabledState
    {
        get
        {
            uint value;
            Disunity.Hosting.DisunityHost.ThrowIfError(Disunity.Native.NativeBindings.disunity_script_enabled(Disunity.Hosting.DisunityHost.WorldHandle,
                Disunity.Hosting.DisunityHost.ScriptIdentity(this), &value));
            return value == 1;
        }
        set
        {
            Disunity.Native.DisunityAdvance work;
            Disunity.Hosting.DisunityHost.ThrowIfError(Disunity.Native.NativeBindings.disunity_script_set_enabled(Disunity.Hosting.DisunityHost.WorldHandle,
                Disunity.Hosting.DisunityHost.ScriptIdentity(this), value ? 1u : 0u, &work));
            Disunity.Hosting.DisunityHost.Drain(work);
        }
    }

    /// <summary>Stored: the engine draws no OnGUI layer. Defaults to true.</summary>
    public bool useGUILayout { get; set; } = true;

    public Coroutine StartCoroutine(System.Collections.IEnumerator iterator) => Disunity.Hosting.DisunityHost.StartCoroutine(this, iterator);
    public void StopCoroutine(Coroutine routine) => Disunity.Hosting.DisunityHost.StopCoroutine(this, routine);
    public void StopCoroutine(System.Collections.IEnumerator iterator) => Disunity.Hosting.DisunityHost.StopCoroutine(this, iterator);
    public void StopAllCoroutines() => Disunity.Hosting.DisunityHost.StopAllCoroutines(this);
}
