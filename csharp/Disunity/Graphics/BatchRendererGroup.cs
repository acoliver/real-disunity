using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;
using Unity.Jobs;

namespace UnityEngine.Rendering;

/// <summary>
/// The meshes, materials and batches a game hands to the renderer, numbered for each group. The engine keeps
/// this registry for the game and does not draw batches yet: no renderer in the engine consumes them, so the
/// culling callback is not invoked on its own. A renderer reaches the callback through <see cref="InvokeCullingCallback"/>.
/// </summary>
public sealed class BatchRendererGroup : IDisposable
{
    /// <summary>The engine's choice for the largest window of a constant buffer a batch can use.</summary>
    private const int ConstantBufferMaxWindowSize = 65536;

    public delegate JobHandle OnPerformCulling(BatchRendererGroup rendererGroup, BatchCullingContext cullingContext, BatchCullingOutput cullingOutput, IntPtr userContext);

    /// <summary>A batch as the group holds it: a copy of its metadata and the part of the buffer it reads.</summary>
    internal sealed record Batch(MetadataValue[] Metadata, GraphicsBufferHandle Buffer, uint BufferOffset, uint WindowSize);

    private readonly object _lock = new();
    private readonly Registrations<Mesh> _meshes = new("mesh", "mesh");
    private readonly Registrations<Material> _materials = new("material", "material");
    private readonly Dictionary<uint, Batch> _batches = new();
    private uint _lastBatch;
    private bool _disposed;

    public BatchRendererGroup(OnPerformCulling cullingCallback, IntPtr userContext)
    {
        ArgumentNullException.ThrowIfNull(cullingCallback);
        CullingCallback = cullingCallback;
        UserContext = userContext;
    }

    internal OnPerformCulling CullingCallback { get; }
    internal IntPtr UserContext { get; }

    public BatchMeshID RegisterMesh(Mesh mesh)
    {
        ArgumentNullException.ThrowIfNull(mesh);
        lock (_lock) return new BatchMeshID { value = Live(_meshes).Register(mesh) };
    }

    public void UnregisterMesh(BatchMeshID mesh)
    {
        lock (_lock) Live(_meshes).Unregister(mesh.value);
    }

    public Mesh? GetRegisteredMesh(BatchMeshID mesh)
    {
        lock (_lock) return Live(_meshes).Get(mesh.value);
    }

    public BatchMaterialID RegisterMaterial(Material material)
    {
        ArgumentNullException.ThrowIfNull(material);
        lock (_lock) return new BatchMaterialID { value = Live(_materials).Register(material) };
    }

    public void UnregisterMaterial(BatchMaterialID material)
    {
        lock (_lock) Live(_materials).Unregister(material.value);
    }

    public Material? GetRegisteredMaterial(BatchMaterialID material)
    {
        lock (_lock) return Live(_materials).Get(material.value);
    }

    /// <summary>Adds a batch that reads the whole buffer. The metadata is copied.</summary>
    public BatchID AddBatch(NativeArray<MetadataValue> batchMetadata, GraphicsBufferHandle buffer) => AddBatch(batchMetadata, buffer, 0, 0);

    /// <summary>Adds a batch that reads a window of the buffer. The metadata is copied.</summary>
    public BatchID AddBatch(NativeArray<MetadataValue> batchMetadata, GraphicsBufferHandle buffer, uint bufferOffset, uint windowSize)
    {
        lock (_lock)
        {
            ThrowIfDisposed();
            if (!batchMetadata.IsCreated) throw new ArgumentException("the batch metadata is not an allocated NativeArray", nameof(batchMetadata));
            if (!GraphicsBuffer.IsLive(buffer)) throw new ArgumentException($"buffer handle {buffer.value} does not belong to a live GraphicsBuffer", nameof(buffer));
            var id = ++_lastBatch;
            _batches.Add(id, new Batch(batchMetadata.ToArray(), buffer, bufferOffset, windowSize));
            return new BatchID { value = id };
        }
    }

    public void RemoveBatch(BatchID batchID)
    {
        lock (_lock)
        {
            ThrowIfDisposed();
            if (!_batches.Remove(batchID.value)) throw new ArgumentException($"batch {batchID.value} is not a batch of this BatchRendererGroup", nameof(batchID));
        }
    }

    internal Batch? GetBatch(BatchID batchID)
    {
        lock (_lock)
        {
            ThrowIfDisposed();
            return _batches.GetValueOrDefault(batchID.value);
        }
    }

    public static int GetConstantBufferMaxWindowSize() => ConstantBufferMaxWindowSize;

    /// <summary>
    /// Runs the culling callback with the context and returns the draw commands it wrote to a one-element output.
    /// The arrays in the result belong to the caller, who releases them with <see cref="FreeDrawCommands"/>.
    /// </summary>
    internal BatchCullingOutputDrawCommands InvokeCullingCallback(BatchCullingContext context)
    {
        lock (_lock) ThrowIfDisposed();
        var drawCommands = new NativeArray<BatchCullingOutputDrawCommands>(1, Allocator.TempJob);
        try
        {
            CullingCallback(this, context, new BatchCullingOutput { drawCommands = drawCommands }, UserContext).Complete();
            return drawCommands[0];
        }
        finally
        {
            drawCommands.Dispose();
        }
    }

    /// <summary>Frees the arrays a culling callback allocated with <c>Allocator.TempJob</c>.</summary>
    internal static unsafe void FreeDrawCommands(BatchCullingOutputDrawCommands commands)
    {
        UnsafeUtility.Free(commands.drawCommands, Allocator.TempJob);
        UnsafeUtility.Free(commands.drawRanges, Allocator.TempJob);
        UnsafeUtility.Free(commands.visibleInstances, Allocator.TempJob);
        UnsafeUtility.Free(commands.instanceSortingPositions, Allocator.TempJob);
    }

    public void Dispose()
    {
        lock (_lock)
        {
            _disposed = true;
            _meshes.Clear();
            _materials.Clear();
            _batches.Clear();
        }
    }

    private Registrations<T> Live<T>(Registrations<T> registrations) where T : class
    {
        ThrowIfDisposed();
        return registrations;
    }

    private void ThrowIfDisposed() => ObjectDisposedException.ThrowIf(_disposed, this);

    /// <summary>
    /// Objects registered with a group, each under an ID numbered from 1 that is not handed out again.
    /// Registering an object that is already registered returns its ID and counts one more registration.
    /// </summary>
    private sealed class Registrations<T>(string kind, string parameter) where T : class
    {
        private readonly Dictionary<uint, (T Item, int Count)> _byId = new();
        private readonly Dictionary<T, uint> _byItem = new(ReferenceEqualityComparer.Instance);
        private uint _last;

        public uint Register(T item)
        {
            if (_byItem.TryGetValue(item, out var existing))
            {
                var entry = _byId[existing];
                _byId[existing] = (entry.Item, entry.Count + 1);
                return existing;
            }
            var id = ++_last;
            _byId.Add(id, (item, 1));
            _byItem.Add(item, id);
            return id;
        }

        public void Unregister(uint id)
        {
            if (!_byId.TryGetValue(id, out var entry)) throw new ArgumentException($"{kind} {id} is not registered with this BatchRendererGroup", parameter);
            if (entry.Count > 1)
            {
                _byId[id] = (entry.Item, entry.Count - 1);
                return;
            }
            _byId.Remove(id);
            _byItem.Remove(entry.Item);
        }

        public T? Get(uint id) => _byId.TryGetValue(id, out var entry) ? entry.Item : null;

        public void Clear()
        {
            _byId.Clear();
            _byItem.Clear();
        }
    }
}
