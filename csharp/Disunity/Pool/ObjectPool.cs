namespace UnityEngine.Pool;

/// <summary>A pool of reusable objects. Like the rest of the engine API it belongs to the main
/// thread and takes no locks, which is the engine's choice.</summary>
public class ObjectPool<T> : IDisposable, IObjectPool<T> where T : class
{
    private readonly Stack<T> _stack;
    private readonly Func<T> _createFunc;
    private readonly Action<T>? _actionOnGet;
    private readonly Action<T>? _actionOnRelease;
    private readonly Action<T>? _actionOnDestroy;
    private readonly int _maxSize;
    private readonly bool _collectionCheck;

    public int CountAll { get; private set; }
    public int CountActive => CountAll - CountInactive;
    public int CountInactive => _stack.Count;

    public ObjectPool(Func<T> createFunc, Action<T>? actionOnGet = null, Action<T>? actionOnRelease = null,
        Action<T>? actionOnDestroy = null, bool collectionCheck = true, int defaultCapacity = 10, int maxSize = 10000)
    {
        ArgumentNullException.ThrowIfNull(createFunc);
        if (maxSize <= 0) { throw new ArgumentException("Max Size must be greater than 0", nameof(maxSize)); }
        _stack = new Stack<T>(defaultCapacity);
        _createFunc = createFunc;
        _maxSize = maxSize;
        _actionOnGet = actionOnGet;
        _actionOnRelease = actionOnRelease;
        _actionOnDestroy = actionOnDestroy;
        _collectionCheck = collectionCheck;
    }

    public T Get()
    {
        T element;
        if (_stack.Count == 0)
        {
            element = _createFunc();
            CountAll++;
        }
        else { element = _stack.Pop(); }
        _actionOnGet?.Invoke(element);
        return element;
    }

    public PooledObject<T> Get(out T value) => new(value = Get(), this);

    /// <summary>Returns an element. With collection checks on, an element already in the pool
    /// throws <see cref="InvalidOperationException"/>. When the pool already holds its maximum
    /// the element is destroyed instead and no longer counts.</summary>
    public void Release(T element)
    {
        if (_collectionCheck && _stack.Contains(element))
        {
            throw new InvalidOperationException("Trying to release an object that has already been released to the pool.");
        }
        _actionOnRelease?.Invoke(element);
        if (CountInactive < _maxSize) { _stack.Push(element); return; }
        CountAll--;
        _actionOnDestroy?.Invoke(element);
    }

    /// <summary>Destroys every pooled element and resets <see cref="CountAll"/>, as active elements
    /// are no longer tracked.</summary>
    public void Clear()
    {
        if (_actionOnDestroy != null)
        {
            foreach (var element in _stack) { _actionOnDestroy(element); }
        }
        _stack.Clear();
        CountAll = 0;
    }

    public void Dispose() => Clear();
}
