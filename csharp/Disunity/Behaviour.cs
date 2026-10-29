namespace UnityEngine;

public abstract class Behaviour : Component
{
    /// <summary>
    /// The enabled state. The property is not virtual, so a derived class that reads or writes
    /// <c>base.enabled</c> (compiled as a non-virtual call) reaches the state of the actual component.
    /// </summary>
    public bool enabled { get => EnabledState; set => EnabledState = value; }

    private protected abstract bool EnabledState { get; set; }

    public bool isActiveAndEnabled => enabled && gameObject.activeInHierarchy;
}
