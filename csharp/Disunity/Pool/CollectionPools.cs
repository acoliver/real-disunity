namespace UnityEngine.Pool;

/// <summary>A shared pool of collections that are cleared when released.</summary>
public class CollectionPool<TCollection, TItem> where TCollection : class, ICollection<TItem>, new()
{
    internal static readonly ObjectPool<TCollection> s_Pool = new(
        () => new TCollection(), null, collection => collection.Clear(), null, true, 10, 10000);

    public static TCollection Get() => s_Pool.Get();

    public static PooledObject<TCollection> Get(out TCollection value) => s_Pool.Get(out value);

    public static void Release(TCollection toRelease) => s_Pool.Release(toRelease);
}

public class ListPool<T> : CollectionPool<List<T>, T> { }

public class HashSetPool<T> : CollectionPool<HashSet<T>, T> { }

public class DictionaryPool<TKey, TValue> : CollectionPool<Dictionary<TKey, TValue>, KeyValuePair<TKey, TValue>>
    where TKey : notnull { }
