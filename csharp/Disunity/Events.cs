namespace UnityEngine.Events;

public delegate void UnityAction<in T>(T value);

[Serializable]
public class UnityEvent<T>
{
    private event UnityAction<T>? Listeners;
    public void AddListener(UnityAction<T> listener) => Listeners += listener;
    public void RemoveListener(UnityAction<T> listener) => Listeners -= listener;
    public void RemoveAllListeners() => Listeners = null;
    public void Invoke(T value) => Listeners?.Invoke(value);
}
