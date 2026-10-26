namespace UnityEngine;

public abstract class Behaviour : Component
{
    public abstract bool enabled { get; set; }
    public bool isActiveAndEnabled => enabled && gameObject.activeInHierarchy;
}
