namespace UnityEngine.Pool;

/// <summary>A shared pool of objects created with their parameterless constructor.</summary>
public static class GenericPool<T> where T : class, new()
{
    private static readonly ObjectPool<T> s_Pool = new(() => new T(), null, null, null, true, 10, 10000);

    public static T Get() => s_Pool.Get();

    public static PooledObject<T> Get(out T value) => s_Pool.Get(out value);

    public static void Release(T toRelease) => s_Pool.Release(toRelease);
}

/// <summary>Like <see cref="GenericPool{T}"/> without the double release check.</summary>
public static class UnsafeGenericPool<T> where T : class, new()
{
    private static readonly ObjectPool<T> s_Pool = new(() => new T(), null, null, null, false, 10, 10000);

    public static T Get() => s_Pool.Get();

    public static PooledObject<T> Get(out T value) => s_Pool.Get(out value);

    public static void Release(T toRelease) => s_Pool.Release(toRelease);
}
