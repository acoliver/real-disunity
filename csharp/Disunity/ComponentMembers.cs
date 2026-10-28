using System.Diagnostics.CodeAnalysis;

namespace UnityEngine;

/// <summary>The component searches of a Component, which run on its GameObject.</summary>
public abstract partial class Component
{
    public Component? GetComponent(Type type) => gameObject.GetComponent(type);
    public bool TryGetComponent<T>([MaybeNullWhen(false)] out T component) => gameObject.TryGetComponent(out component);
    public bool TryGetComponent(Type type, [MaybeNullWhen(false)] out Component component) => gameObject.TryGetComponent(type, out component);

    public Component[] GetComponents(Type type) => gameObject.GetComponents(type);
    public T[] GetComponents<T>() => gameObject.GetComponents<T>();
    public void GetComponents(Type type, List<Component> results) => gameObject.GetComponents(type, results);
    public void GetComponents<T>(List<T> results) => gameObject.GetComponents(results);

    public Component? GetComponentInChildren(Type type) => gameObject.GetComponentInChildren(type);
    public Component? GetComponentInChildren(Type type, bool includeInactive) => gameObject.GetComponentInChildren(type, includeInactive);
    public T? GetComponentInChildren<T>() => gameObject.GetComponentInChildren<T>();
    public T? GetComponentInChildren<T>(bool includeInactive) => gameObject.GetComponentInChildren<T>(includeInactive);

    public Component? GetComponentInParent(Type type) => gameObject.GetComponentInParent(type);
    public Component? GetComponentInParent(Type type, bool includeInactive) => gameObject.GetComponentInParent(type, includeInactive);
    public T? GetComponentInParent<T>() => gameObject.GetComponentInParent<T>();
    public T? GetComponentInParent<T>(bool includeInactive) => gameObject.GetComponentInParent<T>(includeInactive);

    public Component[] GetComponentsInChildren(Type type) => gameObject.GetComponentsInChildren(type);
    public Component[] GetComponentsInChildren(Type type, bool includeInactive) => gameObject.GetComponentsInChildren(type, includeInactive);
    public T[] GetComponentsInChildren<T>() => gameObject.GetComponentsInChildren<T>();
    public T[] GetComponentsInChildren<T>(bool includeInactive) => gameObject.GetComponentsInChildren<T>(includeInactive);
    public void GetComponentsInChildren<T>(List<T> results) => gameObject.GetComponentsInChildren(results);
    public void GetComponentsInChildren<T>(bool includeInactive, List<T> results) => gameObject.GetComponentsInChildren(includeInactive, results);

    public Component[] GetComponentsInParent(Type type) => gameObject.GetComponentsInParent(type);
    public Component[] GetComponentsInParent(Type type, bool includeInactive) => gameObject.GetComponentsInParent(type, includeInactive);
    public T[] GetComponentsInParent<T>() => gameObject.GetComponentsInParent<T>();
    public T[] GetComponentsInParent<T>(bool includeInactive) => gameObject.GetComponentsInParent<T>(includeInactive);
    public void GetComponentsInParent<T>(bool includeInactive, List<T> results) => gameObject.GetComponentsInParent(includeInactive, results);
}
