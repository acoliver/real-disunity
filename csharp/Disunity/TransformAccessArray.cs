namespace UnityEngine.Jobs;

/// <summary>
/// A list of transforms for jobs. The engine keeps the transforms managed-side and runs no transform jobs, so the
/// array is only a list: <c>length</c>, <c>capacity</c>, <c>Add</c>, <c>RemoveAtSwapBack</c>, the indexer and
/// <c>SetTransforms</c>. Copies of the struct share one list, and disposing any copy disposes it for all of them.
/// Every member except <c>isCreated</c> and <c>Dispose</c> of a disposed or default array throws
/// <see cref="InvalidOperationException"/>; disposing such an array throws as well.
/// </summary>
public struct TransformAccessArray : IDisposable
{
    private sealed class Storage(int capacity)
    {
        public readonly List<Transform> Items = new(capacity);
        public int Capacity = capacity;
        public bool Disposed;
    }

    private Storage? _storage;

    public TransformAccessArray(int capacity, int desiredJobCount = -1) => _storage = new Storage(CheckCapacity(capacity));

    public TransformAccessArray(Transform[] transforms, int desiredJobCount = -1) : this(CheckTransforms(transforms).Length, desiredJobCount) =>
        SetTransforms(transforms);

    public static void Allocate(int capacity, int desiredJobCount, out TransformAccessArray array) => array = new TransformAccessArray(capacity, desiredJobCount);

    public readonly bool isCreated => _storage is { Disposed: false };

    public readonly int length => Live().Items.Count;

    public int capacity
    {
        readonly get => Live().Capacity;
        set
        {
            var storage = Live();
            if (value < storage.Items.Count) throw new ArgumentOutOfRangeException(nameof(value), $"capacity {value} is below the length {storage.Items.Count}");
            storage.Capacity = value;
        }
    }

    public Transform this[int index]
    {
        readonly get => Live().Items[CheckIndex(index)];
        set => Live().Items[CheckIndex(index)] = value ?? throw new ArgumentNullException(nameof(value));
    }

    public void Add(Transform transform)
    {
        ArgumentNullException.ThrowIfNull(transform);
        var storage = Live();
        storage.Items.Add(transform);
        if (storage.Items.Count > storage.Capacity) storage.Capacity = Math.Max(storage.Capacity * 2, storage.Items.Count);
    }

    /// <summary>Removes the transform at the index by moving the last one into its place.</summary>
    public void RemoveAtSwapBack(int index)
    {
        var items = Live().Items;
        var last = items.Count - 1;
        items[CheckIndex(index)] = items[last];
        items.RemoveAt(last);
    }

    /// <summary>Replaces the contents; the capacity grows to fit.</summary>
    public void SetTransforms(Transform[] transforms)
    {
        var storage = Live();
        CheckTransforms(transforms);
        storage.Items.Clear();
        storage.Items.AddRange(transforms);
        storage.Capacity = Math.Max(storage.Capacity, transforms.Length);
    }

    public void Dispose()
    {
        var storage = Live();
        storage.Disposed = true;
        storage.Items.Clear();
    }

    private readonly Storage Live() =>
        _storage is { Disposed: false } storage ? storage : throw new InvalidOperationException("The TransformAccessArray is not created or has been disposed");

    private readonly int CheckIndex(int index) =>
        (uint)index < (uint)Live().Items.Count ? index : throw new IndexOutOfRangeException($"index {index} is out of range of '{Live().Items.Count}' length");

    private static int CheckCapacity(int capacity)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(capacity);
        return capacity;
    }

    private static Transform[] CheckTransforms(Transform[] transforms)
    {
        ArgumentNullException.ThrowIfNull(transforms);
        if (Array.IndexOf(transforms, null) >= 0) throw new ArgumentNullException(nameof(transforms), "a transform in the array is null");
        return transforms;
    }
}
