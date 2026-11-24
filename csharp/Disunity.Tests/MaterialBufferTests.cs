using System.Text;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Material.SetBuffer with a compute buffer binds the buffer to the property name of the material the world draws with:
/// a draw of the material makes the graphics snapshot carry the binding and the buffer's contents once, whenever the binding was
/// made. A null buffer, a released buffer and a GraphicsBuffer, which a material does not bind, leave no binding in the world.</summary>
internal static unsafe class MaterialBufferTests
{
    private static readonly byte[] Payload = [0xA1, 0xB2, 0xC3, 0xD4, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC];

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ABufferSetBeforeTheWorldMaterialExistsReachesTheWorld();
            ABufferSetAfterTheWorldMaterialExistsReachesTheWorld();
            ASharedBufferIsCopiedOnce();
            ANullOrReleasedBufferRemovesTheBinding();
            AGraphicsBufferIsNotBoundAndUnbindsTheNameItReplaces();
            ACopiedMaterialCarriesItsBuffers();
            TheWorldRefusesABufferItDoesNotHold();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static ComputeBuffer Filled(byte[]? payload = null)
    {
        var buffer = new ComputeBuffer(4, 4, ComputeBufferType.Structured);
        buffer.SetData(payload ?? Payload);
        return buffer;
    }

    /// <summary>Draws with the materials, then takes the frame's graphics snapshot, which drains the frame's work.</summary>
    private static byte[] SnapshotAfterDrawing(params Material[] materials)
    {
        var mesh = new Mesh();
        mesh.vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 0)];
        mesh.triangles = [0, 1, 2];
        var buffer = new CommandBuffer();
        foreach (var material in materials) buffer.DrawMesh(mesh, Matrix4x4.identity, material);
        Graphics.ExecuteCommandBuffer(buffer);
        buffer.Release();
        var world = DisunityHost.WorldHandle;
        uint length;
        DisunityHost.ThrowIfError(NativeBindings.disunity_graphics_snapshot(world, null, 0, &length));
        var bytes = new byte[length];
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_graphics_snapshot(world, pointer, length, &length));
        return bytes;
    }

    private static int Count(byte[] haystack, byte[] needle)
    {
        var count = 0;
        for (var at = 0; at + needle.Length <= haystack.Length; at++)
            if (haystack.AsSpan(at, needle.Length).SequenceEqual(needle)) count++;
        return count;
    }

    private static int Count(byte[] haystack, string name) => Count(haystack, Encoding.UTF8.GetBytes(name));

    private static void ABufferSetBeforeTheWorldMaterialExistsReachesTheWorld()
    {
        var material = new Material(Material.SpriteLit);
        using var buffer = Filled();
        material.SetBuffer("_Properties", buffer);
        var snapshot = SnapshotAfterDrawing(material);
        Expect.Equal(1, Count(snapshot, "_Properties"), "the binding is named once in the frame's materials");
        Expect.Equal(1, Count(snapshot, Payload), "the buffer's contents are in the frame once");
        Console.WriteLine("PASS: a buffer set on a Material before it has a world material reaches the world with the material");
    }

    private static void ABufferSetAfterTheWorldMaterialExistsReachesTheWorld()
    {
        var material = new Material(Material.SpriteLit);
        _ = material.WorldLink;
        using var buffer = Filled();
        material.SetBuffer(Shader.PropertyToID("_Late"), buffer);
        var snapshot = SnapshotAfterDrawing(material);
        Expect.Equal(1, Count(snapshot, "_Late"), "the binding made after the world material exists");
        Expect.Equal(1, Count(snapshot, Payload), "its contents");
        Console.WriteLine("PASS: a buffer set on a Material that already has a world material reaches the world at once");
    }

    private static void ASharedBufferIsCopiedOnce()
    {
        var (first, second) = (new Material(Material.SpriteLit), new Material(Material.SpriteLit));
        using var buffer = Filled();
        first.SetBuffer("_Properties", buffer);
        second.SetBuffer("_Shared", buffer);
        var snapshot = SnapshotAfterDrawing(first, second);
        Expect.That(Count(snapshot, "_Properties") == 1 && Count(snapshot, "_Shared") == 1, "both materials name the buffer");
        Expect.Equal(1, Count(snapshot, Payload), "two materials binding one buffer carry its contents once");
        Console.WriteLine("PASS: a buffer two materials bind is in the frame's snapshot once");
    }

    private static void ANullOrReleasedBufferRemovesTheBinding()
    {
        var material = new Material(Material.SpriteLit);
        using var kept = Filled();
        material.SetBuffer("_Properties", kept);
        _ = material.WorldLink;
        material.SetBuffer("_Properties", (ComputeBuffer)null!);
        var snapshot = SnapshotAfterDrawing(material);
        Expect.Equal(0, Count(snapshot, "_Properties"), "a null buffer unbinds the name");
        Expect.Equal(0, Count(snapshot, Payload), "and nothing of the buffer reaches the frame");
        var released = Filled();
        material.SetBuffer("_Gone", released);
        released.Release();
        material.SetBuffer("_Gone", released);
        Expect.Equal(0, Count(SnapshotAfterDrawing(material), "_Gone"), "a released buffer binds nothing");
        Console.WriteLine("PASS: SetBuffer with a null or released buffer removes the binding from the world");
    }

    private static void AGraphicsBufferIsNotBoundAndUnbindsTheNameItReplaces()
    {
        var material = new Material(Material.SpriteLit);
        using var compute = Filled();
        using var graphics = new GraphicsBuffer(GraphicsBuffer.Target.Structured, 4, 4);
        material.SetBuffer("_Properties", compute);
        _ = material.WorldLink;
        material.SetBuffer("_Properties", graphics);
        material.SetBuffer("_Managed", graphics);
        Ensure(material.HasBuffer("_Properties") && material.HasBuffer("_Managed"), "the material still reports both buffers");
        var snapshot = SnapshotAfterDrawing(material);
        Expect.Equal(0, Count(snapshot, "_Properties"), "a GraphicsBuffer replaces the compute buffer's binding");
        Expect.Equal(0, Count(snapshot, "_Managed"), "a material does not bind a GraphicsBuffer");
        Expect.Equal(0, Count(snapshot, Payload), "nothing of either buffer reaches the frame");
        Console.WriteLine("PASS: a GraphicsBuffer set on a Material is kept by the material, not bound, and unbinds the name it replaces");
    }

    private static void ACopiedMaterialCarriesItsBuffers()
    {
        var source = new Material(Material.SpriteLit);
        using var buffer = Filled();
        source.SetBuffer("_Properties", buffer);
        var copy = new Material(source);
        var snapshot = SnapshotAfterDrawing(copy);
        Expect.Equal(1, Count(snapshot, "_Properties"), "the copy binds the name");
        Expect.Equal(1, Count(snapshot, Payload), "to the same buffer");
        var target = new Material(Material.SpriteLit);
        _ = target.WorldLink;
        target.CopyPropertiesFromMaterial(source);
        Expect.Equal(1, Count(SnapshotAfterDrawing(target), Payload), "CopyPropertiesFromMaterial sends the buffers to an existing world material");
        Console.WriteLine("PASS: a material made from another, or given its properties, binds the same buffers in the world");
    }

    private static void TheWorldRefusesABufferItDoesNotHold()
    {
        var material = new Material(Material.SpriteLit);
        var (world, id, name) = (DisunityHost.WorldHandle, material.WorldLink.Id, Encoding.UTF8.GetBytes("_Properties"));
        fixed (byte* pointer = name)
        {
            var unknown = NativeBindings.disunity_material_set_buffer(world, id, pointer, (uint)name.Length, 0x0000_7777_0000_7777);
            Ensure(unknown.code != 0, "a buffer id the world never issued is refused");
            var unbound = NativeBindings.disunity_material_set_buffer(world, id, pointer, (uint)name.Length, 0);
            Expect.Equal(0, unbound.code, "buffer 0 removes a binding that is not there");
        }
        Console.WriteLine("PASS: disunity_material_set_buffer refuses a buffer the world does not hold");
    }
}
