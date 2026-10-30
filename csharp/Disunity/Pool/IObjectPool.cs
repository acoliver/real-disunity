namespace UnityEngine.Pool;

public interface IObjectPool<T> where T : class
{
    int CountInactive { get; }
    T Get();
    PooledObject<T> Get(out T value);
    void Release(T element);
    void Clear();
}

/// <summary>A handle that returns its element to the pool it came from when disposed. Disposing a
/// default handle does nothing.</summary>
public readonly struct PooledObject<T> : IDisposable where T : class
{
    private readonly T? _value;
    private readonly IObjectPool<T>? _pool;

    internal PooledObject(T value, IObjectPool<T> pool)
    {
        _value = value;
        _pool = pool;
    }

    public void Dispose()
    {
        if (_pool != null && _value != null) { _pool.Release(_value); }
    }
}
