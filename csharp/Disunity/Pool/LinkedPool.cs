namespace UnityEngine.Pool;

/// <summary>A pool that keeps its idle elements in a linked list, so releasing never allocates
/// beyond the node.</summary>
public class LinkedPool<T> : IDisposable, IObjectPool<T> where T : class
{
    private readonly LinkedList<T> _list = new();
    private readonly Func<T> _createFunc;
    private readonly Action<T>? _actionOnGet;
    private readonly Action<T>? _actionOnRelease;
    private readonly Action<T>? _actionOnDestroy;
    private readonly int _maxSize;
    private readonly bool _collectionCheck;

    public int CountAll { get; private set; }
    public int CountActive => CountAll - CountInactive;
    public int CountInactive => _list.Count;

    public LinkedPool(Func<T> createFunc, Action<T>? actionOnGet = null, Action<T>? actionOnRelease = null,
        Action<T>? actionOnDestroy = null, bool collectionCheck = true, int maxSize = 10000)
    {
        ArgumentNullException.ThrowIfNull(createFunc);
        if (maxSize <= 0) { throw new ArgumentException("Max Size must be greater than 0", nameof(maxSize)); }
        _createFunc = createFunc;
        _actionOnGet = actionOnGet;
        _actionOnRelease = actionOnRelease;
        _actionOnDestroy = actionOnDestroy;
        _maxSize = maxSize;
        _collectionCheck = collectionCheck;
    }

    public T Get()
    {
        T element;
        if (_list.Count == 0)
        {
            element = _createFunc();
            CountAll++;
        }
        else
        {
            element = _list.Last!.Value;
            _list.RemoveLast();
        }
        _actionOnGet?.Invoke(element);
        return element;
    }

    public PooledObject<T> Get(out T value) => new(value = Get(), this);

    public void Release(T element)
    {
        if (_collectionCheck && _list.Contains(element))
        {
            throw new InvalidOperationException("Trying to release an object that has already been released to the pool.");
        }
        _actionOnRelease?.Invoke(element);
        if (CountInactive < _maxSize) { _list.AddLast(element); return; }
        CountAll--;
        _actionOnDestroy?.Invoke(element);
    }

    public void Clear()
    {
        if (_actionOnDestroy != null)
        {
            foreach (var element in _list) { _actionOnDestroy(element); }
        }
        _list.Clear();
        CountAll = 0;
    }

    public void Dispose() => Clear();
}
