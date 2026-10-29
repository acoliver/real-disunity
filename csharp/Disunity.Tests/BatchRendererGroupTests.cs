using Disunity.Hosting;
using Unity.Collections;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>BatchRendererGroup's registry of meshes, materials and batches, and the ID structs it hands out.</summary>
internal static class BatchRendererGroupTests
{
    private sealed record Registry<TObject, TId>(
        Func<BatchRendererGroup, TObject, TId> Register,
        Action<BatchRendererGroup, TId> Unregister,
        Func<BatchRendererGroup, TId, TObject?> Get,
        Func<TObject> Make,
        Func<uint, TId> IdOf,
        Func<TId, uint> ValueOf) where TObject : class;

    private static readonly Registry<Mesh, BatchMeshID> Meshes = new(
        (group, mesh) => group.RegisterMesh(mesh), (group, id) => group.UnregisterMesh(id), (group, id) => group.GetRegisteredMesh(id),
        () => new Mesh(), value => new BatchMeshID { value = value }, id => id.value);

    private static readonly Registry<Material, BatchMaterialID> Materials = new(
        (group, material) => group.RegisterMaterial(material), (group, id) => group.UnregisterMaterial(id), (group, id) => group.GetRegisteredMaterial(id),
        () => new Material(Material.SpriteUnlit), value => new BatchMaterialID { value = value }, id => id.value);

    private static BatchRendererGroup NewGroup() => new((_, _, _, _) => default, IntPtr.Zero);

    public static void Run()
    {
        KeepsTheCallbackAndContext();
        RegistersAndUnregistersEachKindOfObject();
        IdStructsCompareAndHash();
        AddsBatchesOnlyForLiveBuffers();
        CopiesMetadataAndKeepsTheWindow();
        RemovesBatches();
        ReportsTheConstantBufferWindow();
        RejectsEveryMemberAfterDispose();
        ModuleFacadesForwardTheTypes();
        Console.WriteLine("PASS: BatchRendererGroup callback, mesh, material and batch registry, ID structs and dispose");
    }

    private static void KeepsTheCallbackAndContext()
    {
        Expect.Throws<ArgumentNullException>(() => new BatchRendererGroup(null!, IntPtr.Zero), "a null culling callback");
        BatchRendererGroup.OnPerformCulling callback = (_, _, _, _) => default;
        using var group = new BatchRendererGroup(callback, (IntPtr)42);
        Expect.That(ReferenceEquals(group.CullingCallback, callback) && group.UserContext == (IntPtr)42, "the callback and user context are kept");
    }

    private static void RegistersAndUnregistersEachKindOfObject()
    {
        RegistryRules(Meshes, "mesh");
        RegistryRules(Materials, "material");
    }

    private static void RegistryRules<TObject, TId>(Registry<TObject, TId> registry, string kind) where TObject : class
    {
        using var group = NewGroup();
        var first = registry.Make();
        var second = registry.Make();
        var firstId = registry.Register(group, first);
        var secondId = registry.Register(group, second);
        Expect.That(registry.ValueOf(firstId) == 1 && registry.ValueOf(secondId) == 2, $"{kind} IDs start at 1 and count up");
        Expect.That(ReferenceEquals(registry.Get(group, firstId), first) && ReferenceEquals(registry.Get(group, secondId), second), $"{kind} lookup returns the registered object");
        Expect.That(registry.ValueOf(registry.Register(group, first)) == 1, $"registering the same {kind} again returns its ID");

        registry.Unregister(group, firstId);
        Expect.That(ReferenceEquals(registry.Get(group, firstId), first), $"a {kind} registered twice survives one unregister");
        registry.Unregister(group, firstId);
        Expect.That(registry.Get(group, firstId) == null, $"the second unregister frees the {kind} ID");
        Expect.Throws<ArgumentException>(() => registry.Unregister(group, firstId), $"unregistering a freed {kind} ID");
        Expect.Throws<ArgumentException>(() => registry.Unregister(group, registry.IdOf(99)), $"unregistering an unknown {kind} ID");
        Expect.Throws<ArgumentException>(() => registry.Unregister(group, registry.IdOf(0)), $"unregistering the null {kind} ID");
        Expect.That(registry.Get(group, registry.IdOf(99)) == null && registry.Get(group, registry.IdOf(0)) == null, $"an unknown {kind} ID has no object");
        Expect.That(ReferenceEquals(registry.Get(group, secondId), second), $"freeing one {kind} leaves the others");
        Expect.Throws<ArgumentNullException>(() => registry.Register(group, null!), $"registering a null {kind}");

        using var other = NewGroup();
        Expect.That(registry.ValueOf(registry.Register(other, second)) == 1, $"{kind} IDs are numbered for each group");
        Expect.That(ReferenceEquals(registry.Get(other, registry.IdOf(1)), second) && registry.Get(other, secondId) == null, $"a group only knows its own {kind} IDs");
    }

    private static void IdStructsCompareAndHash()
    {
        var one = new BatchMeshID { value = 1 };
        var two = new BatchMeshID { value = 2 };
        Expect.That(BatchMeshID.Null.value == 0 && one == new BatchMeshID { value = 1 } && one != two, "mesh ID equality and Null");
        Expect.That(one.Equals((object)new BatchMeshID { value = 1 }) && !one.Equals((object)two) && !one.Equals("1"), "mesh ID object equality");
        Expect.That(one.GetHashCode() == new BatchMeshID { value = 1 }.GetHashCode() && one.CompareTo(two) < 0 && two.CompareTo(one) > 0 && one.CompareTo(one) == 0, "mesh ID hash and order");
        var materials = new BatchMaterialID { value = 5 };
        Expect.That(BatchMaterialID.Null.value == 0 && materials == new BatchMaterialID { value = 5 } && materials != BatchMaterialID.Null, "material ID equality and Null");
        Expect.That(materials.GetHashCode() == new BatchMaterialID { value = 5 }.GetHashCode() && materials.CompareTo(BatchMaterialID.Null) > 0, "material ID hash and order");
        var batch = new BatchID { value = 9 };
        Expect.That(BatchID.Null.value == 0 && batch == new BatchID { value = 9 } && batch != BatchID.Null, "batch ID equality and Null");
        Expect.That(batch.GetHashCode() == new BatchID { value = 9 }.GetHashCode() && batch.CompareTo(BatchID.Null) > 0, "batch ID hash and order");
        var keys = new Dictionary<(BatchMeshID, BatchMaterialID), int> { [(one, materials)] = 3 };
        Expect.That(keys[(new BatchMeshID { value = 1 }, new BatchMaterialID { value = 5 })] == 3, "IDs work as tuple keys");
    }

    private static NativeArray<MetadataValue> Metadata(params (int name, uint value)[] values)
    {
        var array = new NativeArray<MetadataValue>(values.Length, Allocator.Temp);
        for (var i = 0; i < values.Length; i++) array[i] = new MetadataValue { NameID = values[i].name, Value = values[i].value };
        return array;
    }

    private static void AddsBatchesOnlyForLiveBuffers()
    {
        using var group = NewGroup();
        using var live = new GraphicsBuffer(GraphicsBuffer.Target.Raw, 4, 4);
        var released = new GraphicsBuffer(GraphicsBuffer.Target.Raw, 4, 4);
        var releasedHandle = released.bufferHandle;
        released.Release();
        var metadata = Metadata((1, 0));

        var first = group.AddBatch(metadata, live.bufferHandle);
        var second = group.AddBatch(metadata, live.bufferHandle);
        Expect.That(first.value == 1 && second.value == 2 && first != second, "batch IDs start at 1 and count up");
        Expect.Throws<ArgumentException>(() => group.AddBatch(metadata, releasedHandle), "a released buffer's handle");
        Expect.Throws<ArgumentException>(() => group.AddBatch(metadata, new GraphicsBufferHandle { value = uint.MaxValue }), "an unknown handle");
        Expect.Throws<ArgumentException>(() => group.AddBatch(metadata, default), "the zero handle");
        Expect.Throws<ArgumentException>(() => group.AddBatch(default, live.bufferHandle), "a default NativeArray");
        Expect.Throws<ArgumentException>(() => group.AddBatch(default, live.bufferHandle, 0, 16), "a default NativeArray in the four argument form");
        Expect.Throws<ArgumentException>(() => group.AddBatch(metadata, releasedHandle, 0, 16), "a released handle in the four argument form");
        Expect.That(group.GetBatch(group.AddBatch(metadata, live.bufferHandle)) != null, "a rejected call does not stop later batches");
        metadata.Dispose();
    }

    private static void CopiesMetadataAndKeepsTheWindow()
    {
        using var group = NewGroup();
        using var buffer = new GraphicsBuffer(GraphicsBuffer.Target.Raw, 64, 4);
        var metadata = Metadata((10, 100), (20, 200));
        var plain = group.AddBatch(metadata, buffer.bufferHandle);
        var windowed = group.AddBatch(metadata, buffer.bufferHandle, 16, 64);
        metadata[0] = new MetadataValue { NameID = 99, Value = 999 };
        metadata.Dispose();

        var batch = group.GetBatch(plain)!;
        Expect.That(batch.Metadata.Length == 2 && batch.Metadata[0].NameID == 10 && batch.Metadata[0].Value == 100 && batch.Metadata[1].NameID == 20 && batch.Metadata[1].Value == 200,
            "the metadata was copied when the batch was added");
        Expect.That(batch.Buffer == buffer.bufferHandle && batch.BufferOffset == 0 && batch.WindowSize == 0, "the short form uses the whole buffer");
        var window = group.GetBatch(windowed)!;
        Expect.That(window.Buffer == buffer.bufferHandle && window.BufferOffset == 16 && window.WindowSize == 64, "the long form keeps the offset and window size");
    }

    private static void RemovesBatches()
    {
        using var group = NewGroup();
        using var buffer = new GraphicsBuffer(GraphicsBuffer.Target.Raw, 4, 4);
        var metadata = Metadata((1, 0));
        var kept = group.AddBatch(metadata, buffer.bufferHandle);
        var removed = group.AddBatch(metadata, buffer.bufferHandle);
        metadata.Dispose();
        group.RemoveBatch(removed);
        Expect.That(group.GetBatch(removed) == null && group.GetBatch(kept) != null, "RemoveBatch frees only that batch");
        Expect.Throws<ArgumentException>(() => group.RemoveBatch(removed), "removing a removed batch");
        Expect.Throws<ArgumentException>(() => group.RemoveBatch(new BatchID { value = 99 }), "removing an unknown batch");
        Expect.Throws<ArgumentException>(() => group.RemoveBatch(BatchID.Null), "removing the null batch");
    }

    private static void ModuleFacadesForwardTheTypes()
    {
        var engine = typeof(BatchRendererGroup).Assembly;
        string[] names =
        [
            "UnityEngine.Rendering.BatchRendererGroup", "UnityEngine.Rendering.BatchRendererGroup+OnPerformCulling",
            "UnityEngine.Rendering.BatchMeshID", "UnityEngine.Rendering.BatchMaterialID", "UnityEngine.Rendering.BatchID",
            "UnityEngine.Rendering.MetadataValue", "UnityEngine.Rendering.BatchCullingContext", "UnityEngine.Rendering.BatchCullingOutput",
            "UnityEngine.Rendering.BatchCullingOutputDrawCommands", "UnityEngine.Rendering.BatchDrawCommand", "UnityEngine.Rendering.BatchDrawRange",
            "UnityEngine.Rendering.BatchFilterSettings", "UnityEngine.GraphicsBuffer", "UnityEngine.GraphicsBuffer+Target",
        ];
        foreach (var name in names) Expect.That(ModuleFacades.Forwards(engine, name), $"{name} is public in the engine, so a module facade forwards it");
        Expect.That(!ModuleFacades.Forwards(engine, "UnityEngine.Rendering.BatchRendererGroup+Batch"), "the registry's internal batch record is not forwarded");
    }

    private static void ReportsTheConstantBufferWindow() =>
        Expect.Equal(65536, BatchRendererGroup.GetConstantBufferMaxWindowSize(), "the engine's constant buffer window size");

    private static void RejectsEveryMemberAfterDispose()
    {
        var group = NewGroup();
        using var buffer = new GraphicsBuffer(GraphicsBuffer.Target.Raw, 4, 4);
        var metadata = Metadata((1, 0));
        var mesh = group.RegisterMesh(new Mesh());
        var material = group.RegisterMaterial(new Material(Material.SpriteUnlit));
        var batch = group.AddBatch(metadata, buffer.bufferHandle);
        group.Dispose();
        group.Dispose();

        Expect.Throws<ObjectDisposedException>(() => group.RegisterMesh(new Mesh()), "RegisterMesh");
        Expect.Throws<ObjectDisposedException>(() => group.UnregisterMesh(mesh), "UnregisterMesh");
        Expect.Throws<ObjectDisposedException>(() => group.GetRegisteredMesh(mesh), "GetRegisteredMesh");
        Expect.Throws<ObjectDisposedException>(() => group.RegisterMaterial(new Material(Material.SpriteUnlit)), "RegisterMaterial");
        Expect.Throws<ObjectDisposedException>(() => group.UnregisterMaterial(material), "UnregisterMaterial");
        Expect.Throws<ObjectDisposedException>(() => group.GetRegisteredMaterial(material), "GetRegisteredMaterial");
        Expect.Throws<ObjectDisposedException>(() => group.AddBatch(metadata, buffer.bufferHandle), "AddBatch");
        Expect.Throws<ObjectDisposedException>(() => group.AddBatch(metadata, buffer.bufferHandle, 0, 16), "AddBatch with a window");
        Expect.Throws<ObjectDisposedException>(() => group.RemoveBatch(batch), "RemoveBatch");
        Expect.Throws<ObjectDisposedException>(() => group.GetBatch(batch), "GetBatch");
        Expect.Throws<ObjectDisposedException>(() => group.InvokeCullingCallback(default), "the culling callback seam");
        Expect.Equal(65536, BatchRendererGroup.GetConstantBufferMaxWindowSize(), "the static window size needs no group");
        metadata.Dispose();
    }
}
