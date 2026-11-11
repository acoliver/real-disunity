using System.Buffers.Binary;
using System.Globalization;
using System.Text;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Global vector and matrix arrays through the public API: what the world's global state and its ordered offscreen
/// work receive, read back from the native snapshot and the work detail the way a host reads them.</summary>
internal static unsafe class GlobalArrayTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            StaticArraysReachTheWorldInElementOrder();
            Console.WriteLine("PASS: Shader.SetGlobalVectorArray and SetGlobalMatrixArray send every element to the world's global state, matrices column by column");
            StaticArraysAreCopiedAndRefusedWithoutChange();
            Console.WriteLine("PASS: a static array is copied at the call, and a null, empty, oversized, non-finite or badly named one throws and changes nothing");
            FloatArraysNameThemselves();
            Console.WriteLine("PASS: Shader.SetGlobalFloatArray and CommandBuffer.SetGlobalFloatArray throw NotSupportedException naming the member");
            EveryCommandBufferOverloadRecordsInOrder();
            Console.WriteLine("PASS: all eight CommandBuffer vector and matrix array overloads record in order, once per execution, apart from the global state");
            RecordedArraysAreSnapshotsOfTheCall();
            Console.WriteLine("PASS: a recorded array is a copy: later changes to the array or list, Clear, re-recording and Release do not change what is sent");
            ContextDefersArraysUntilSubmit();
            Console.WriteLine("PASS: a context holds recorded arrays until Submit and sends the same records as Graphics.ExecuteCommandBuffer");
            CommandBufferRefusesBadArraysBeforeRecording();
            Console.WriteLine("PASS: a bad name, id, array or component throws at the call and records nothing; a released buffer throws first");
            StaticArraysRefuseAnUnregisteredId();
            Console.WriteLine("PASS: a static array under an id no name was registered for throws ArgumentException and neither reaches the world nor is stored");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
        NativeRefusalLeavesNoManagedCopy();
        Console.WriteLine("PASS: a native refusal of a valid static array (the world's list of that kind is full at 1024 names) throws a capacity error and leaves the stored vectors and matrices and the world as they were");
        ArraysSetBeforeLoadAreSentWhenTheWorldLoads();
        Console.WriteLine("PASS: arrays set while no world is loaded are sent to the global state at load and again at reload, materials send none, and an unregistered id is refused with and without a world and never stored or sent");
    }

    private static readonly System.Globalization.CultureInfo Invariant = CultureInfo.InvariantCulture;

    private static Vector4[] Vectors(int count, float baseValue) =>
        Enumerable.Range(0, count).Select(i => new Vector4(baseValue + i, baseValue + i + 0.25f, -baseValue - i, 1f)).ToArray();

    /// <summary>A matrix whose element (row, column) is <c>base + 10 * row + column</c>, so row-major and column-major order differ in every position.</summary>
    private static Matrix4x4 Matrix(float baseValue)
    {
        var matrix = new Matrix4x4();
        for (var row = 0; row < 4; row++)
            for (var column = 0; column < 4; column++) matrix[row, column] = baseValue + 10 * row + column;
        return matrix;
    }

    private static Matrix4x4[] Matrices(int count, float baseValue) => Enumerable.Range(0, count).Select(i => Matrix(baseValue + 100 * i)).ToArray();

    private static float[] Floats(Vector4 v) => [v.x, v.y, v.z, v.w];

    /// <summary>The sixteen floats of a matrix in the order the world holds them: element <c>column * 4 + row</c>.</summary>
    private static float[] Columns(Matrix4x4 m)
    {
        var values = new float[16];
        for (var column = 0; column < 4; column++)
            for (var row = 0; row < 4; row++) values[column * 4 + row] = m[row, column];
        return values;
    }

    private static byte[] Entry(string name, IEnumerable<float[]> elements)
    {
        var list = elements.ToArray();
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);
        var bytes = Encoding.UTF8.GetBytes(name);
        writer.Write((uint)bytes.Length);
        writer.Write(bytes);
        writer.Write((uint)list.Length);
        foreach (var element in list)
            foreach (var value in element) writer.Write(value);
        writer.Flush();
        return stream.ToArray();
    }

    private static byte[] VectorEntry(string name, Vector4[] values) => Entry(name, values.Select(Floats));

    private static byte[] MatrixEntry(string name, Matrix4x4[] values) => Entry(name, values.Select(Columns));

    private static string Element(float[] floats) => "(" + string.Join(" ", floats.Select(f => f.ToString("R", Invariant))) + ")";

    private static string VectorLine(string name, Vector4[] values) => $"SetGlobal {name}=vectors [{values.Length}] {string.Join(" ", values.Select(v => Element(Floats(v))))}";

    private static string MatrixLine(string name, Matrix4x4[] values) => $"SetGlobal {name}=matrices [{values.Length}] {string.Join(" ", values.Select(m => Element(Columns(m))))}";

    /// <summary>Records a keyword so the frame has work, which makes the next snapshot carry the global state.</summary>
    private static void Trigger()
    {
        var buffer = new CommandBuffer();
        buffer.EnableShaderKeyword("_GA_TRIGGER");
        Graphics.ExecuteCommandBuffer(buffer);
        buffer.Release();
    }

    /// <summary>The native snapshot of the frame, which also drains the frame's work.</summary>
    private static byte[] Snapshot()
    {
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

    /// <summary>Whether the entry sits in the snapshot's ordered work, which puts the work and property tags before it.</summary>
    private static bool InWork(byte[] snapshot, byte[] entry, byte propertyTag)
    {
        var at = snapshot.AsSpan().IndexOf(entry);
        return at >= 2 && snapshot[at - 2] == 6 && snapshot[at - 1] == propertyTag;
    }

    private static void StaticArraysReachTheWorldInElementOrder()
    {
        var vectors = Vectors(3, 1f);
        var matrices = Matrices(2, 1f);
        Shader.SetGlobalVectorArray("_GaVec", vectors);
        Shader.SetGlobalMatrixArray("_GaMat", matrices);
        Trigger();
        var snapshot = Snapshot();
        Ensure(Count(snapshot, VectorEntry("_GaVec", vectors)) == 1, "the vectors sit in the global state in element order");
        Ensure(Count(snapshot, MatrixEntry("_GaMat", matrices)) == 1, "the matrices sit in the global state column by column");
        Ensure(!InWork(snapshot, VectorEntry("_GaVec", vectors), 5) && !InWork(snapshot, MatrixEntry("_GaMat", matrices), 6), "a static setter records no ordered work");
        var rowMajor = Entry("_GaMat", matrices.Select(m => Enumerable.Range(0, 16).Select(i => m[i / 4, i % 4]).ToArray()));
        Ensure(Count(snapshot, rowMajor) == 0, "the transposed order is not what the world holds");
        Ensure(Shader.GetGlobalVectorArray("_GaVec")!.SequenceEqual(vectors) && Shader.GetGlobalMatrixArray("_GaMat")!.SequenceEqual(matrices), "the arrays read back");

        var replacement = Vectors(1, 50f);
        Shader.SetGlobalVectorArray(Shader.PropertyToID("_GaVec"), new List<Vector4>(replacement));
        Trigger();
        snapshot = Snapshot();
        Ensure(Count(snapshot, VectorEntry("_GaVec", replacement)) == 1 && Count(snapshot, VectorEntry("_GaVec", vectors)) == 0, "a shorter array replaces the longer one without a tail");
    }

    private static void StaticArraysAreCopiedAndRefusedWithoutChange()
    {
        var vectors = Vectors(2, 7f);
        var matrices = Matrices(2, 7f);
        Shader.SetGlobalVectorArray("_GaCopyVec", vectors);
        Shader.SetGlobalMatrixArray("_GaCopyMat", new List<Matrix4x4>(matrices));
        var (expectedVectors, expectedMatrices) = ((Vector4[])vectors.Clone(), (Matrix4x4[])matrices.Clone());
        vectors[0] = Vector4.zero;
        matrices[1] = Matrix4x4.zero;
        Shader.GetGlobalVectorArray("_GaCopyVec")![1] = Vector4.zero;
        Trigger();
        var snapshot = Snapshot();
        Ensure(Count(snapshot, VectorEntry("_GaCopyVec", expectedVectors)) == 1 && Count(snapshot, MatrixEntry("_GaCopyMat", expectedMatrices)) == 1, "the world holds the values of the call");
        Ensure(Shader.GetGlobalVectorArray("_GaCopyVec")!.SequenceEqual(expectedVectors), "the stored copy is unchanged by the source and by a read");

        var bad = new (string What, Action Call, Type Error)[]
        {
            ("null vectors", () => Shader.SetGlobalVectorArray("_GaCopyVec", (Vector4[])null!), typeof(ArgumentNullException)),
            ("null matrix list", () => Shader.SetGlobalMatrixArray("_GaCopyMat", (List<Matrix4x4>)null!), typeof(ArgumentNullException)),
            ("empty vectors", () => Shader.SetGlobalVectorArray("_GaCopyVec", Array.Empty<Vector4>()), typeof(ArgumentException)),
            ("1024 matrices", () => Shader.SetGlobalMatrixArray("_GaCopyMat", new Matrix4x4[1024]), typeof(ArgumentException)),
            ("a NaN vector", () => Shader.SetGlobalVectorArray("_GaCopyVec", new[] { Vector4.one, new Vector4(0, 0, 0, float.NaN) }), typeof(ArgumentException)),
            ("an infinite matrix", () => Shader.SetGlobalMatrixArray("_GaCopyMat", new[] { WithElement(float.NegativeInfinity) }), typeof(ArgumentException)),
            ("a name of 257 bytes", () => Shader.SetGlobalVectorArray(new string('n', 257), Vectors(1, 1f)), typeof(ArgumentException)),
            ("a name of 129 two-byte characters", () => Shader.SetGlobalMatrixArray(new string('\u00e9', 129), Matrices(1, 1f)), typeof(ArgumentException)),
        };
        foreach (var (what, call, error) in bad)
        {
            var thrown = Catch(call, what);
            Ensure(thrown.GetType() == error, $"{what}: expected {error.Name}, got {thrown.GetType().Name}");
        }
        Shader.SetGlobalVectorArray("_GaMax", Vectors(1023, 1f));
        Shader.SetGlobalMatrixArray(new string('n', 256), Matrices(1, 1f));
        Trigger();
        var after = Snapshot();
        Ensure(Count(after, VectorEntry("_GaCopyVec", expectedVectors)) == 1 && Count(after, MatrixEntry("_GaCopyMat", expectedMatrices)) == 1, "every refusal left the world and the stored arrays as they were");
        Ensure(Count(after, VectorEntry("_GaMax", Vectors(1023, 1f))) == 1, "1023 elements are accepted");
    }

    private static Matrix4x4 WithElement(float value)
    {
        var matrix = Matrix4x4.identity;
        matrix[2, 3] = value;
        return matrix;
    }

    private static Exception Catch(Action call, string what)
    {
        try { call(); }
        catch (Exception error) { return error; }
        throw new InvalidOperationException($"{what}: nothing was thrown");
    }

    private static void FloatArraysNameThemselves()
    {
        var id = Shader.PropertyToID("_GaFloats");
        var calls = new (string Member, Action Call)[]
        {
            ("SetGlobalFloatArray", () => Shader.SetGlobalFloatArray("_GaFloats", new[] { 1f })),
            ("SetGlobalFloatArray", () => Shader.SetGlobalFloatArray("_GaFloats", new List<float> { 1f })),
            ("SetGlobalFloatArray", () => Shader.SetGlobalFloatArray(id, new[] { 1f })),
            ("SetGlobalFloatArray", () => Shader.SetGlobalFloatArray(id, new List<float> { 1f })),
        };
        foreach (var (member, call) in calls)
            Throws<NotSupportedException>(call, $"Shader.{member}", $"Shader.{member}", "float arrays are not supported");
        Ensure(Shader.GetGlobalFloatArray("_GaFloats") == null, "a refused float array stores nothing");
        var buffer = new CommandBuffer();
        Throws<NotSupportedException>(() => buffer.SetGlobalFloatArray("_GaFloats", new[] { 1f }), "CommandBuffer float array", "CommandBuffer.SetGlobalFloatArray");
        Ensure(buffer.commandCount == 0, "the refused float array recorded nothing");
        buffer.Release();
    }

    private static CommandBuffer RecordEveryOverload(Vector4[] vectors, Matrix4x4[] matrices)
    {
        var buffer = new CommandBuffer();
        var (vectorId, matrixId) = (Shader.PropertyToID("_GaCbVecId"), Shader.PropertyToID("_GaCbMatId"));
        buffer.SetGlobalVectorArray("_GaCbVec", vectors);
        buffer.SetGlobalVectorArray("_GaCbVecList", new List<Vector4>(vectors));
        buffer.SetGlobalVectorArray(vectorId, vectors);
        buffer.SetGlobalVectorArray(vectorId, new List<Vector4>(vectors.Take(1)));
        buffer.SetGlobalMatrixArray("_GaCbMat", matrices);
        buffer.SetGlobalMatrixArray("_GaCbMatList", new List<Matrix4x4>(matrices));
        buffer.SetGlobalMatrixArray(matrixId, matrices);
        buffer.SetGlobalMatrixArray(matrixId, new List<Matrix4x4>(matrices.Take(1)));
        return buffer;
    }

    private static string[] ExpectedLines(Vector4[] vectors, Matrix4x4[] matrices) =>
    [
        VectorLine("_GaCbVec", vectors), VectorLine("_GaCbVecList", vectors), VectorLine("_GaCbVecId", vectors), VectorLine("_GaCbVecId", vectors.Take(1).ToArray()),
        MatrixLine("_GaCbMat", matrices), MatrixLine("_GaCbMatList", matrices), MatrixLine("_GaCbMatId", matrices), MatrixLine("_GaCbMatId", matrices.Take(1).ToArray()),
    ];

    private static void EveryCommandBufferOverloadRecordsInOrder()
    {
        var (vectors, matrices) = (Vectors(3, 2f), Matrices(2, 2f));
        var expected = ExpectedLines(vectors, matrices);
        CommandWireReader.Take();
        var buffer = RecordEveryOverload(vectors, matrices);
        Ensure(buffer.commandCount == 8 && CommandWireReader.Take().Length == 0, "recording sends the world nothing");
        Graphics.ExecuteCommandBuffer(buffer);
        var first = CommandWireReader.Take();
        Ensure(first.SequenceEqual(expected), $"the world received:\n{string.Join("\n", first)}\nexpected:\n{string.Join("\n", expected)}");
        Graphics.ExecuteCommandBuffer(buffer);
        Ensure(CommandWireReader.Take().SequenceEqual(expected), "executing an unchanged buffer again delivers the same records again");

        Ensure(Shader.GetGlobalVectorArray("_GaCbVec") == null && Shader.GetGlobalMatrixArray("_GaCbMat") == null, "a command buffer array is not a managed global");
        Graphics.ExecuteCommandBuffer(buffer);
        var snapshot = Snapshot();
        Ensure(InWork(snapshot, VectorEntry("_GaCbVec", vectors), 5) && InWork(snapshot, MatrixEntry("_GaCbMat", matrices), 6), "the ordered records are work");
        Ensure(Count(snapshot, VectorEntry("_GaCbVec", vectors)) == 1, "the world's initial global state does not hold an ordered array");
        buffer.Release();
    }

    private static void RecordedArraysAreSnapshotsOfTheCall()
    {
        var (vectors, matrices) = (Vectors(2, 3f), Matrices(2, 3f));
        var (heldVectors, heldMatrices) = ((Vector4[])vectors.Clone(), (Matrix4x4[])matrices.Clone());
        var vectorList = new List<Vector4>(vectors);
        var buffer = new CommandBuffer();
        buffer.SetGlobalVectorArray("_GaSnapVec", vectors);
        buffer.SetGlobalVectorArray("_GaSnapList", vectorList);
        buffer.SetGlobalMatrixArray("_GaSnapMat", matrices);
        vectors[0] = Vector4.zero;
        vectorList.Clear();
        matrices[0] = Matrix4x4.zero;
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        var sent = CommandWireReader.Take();
        Ensure(sent.SequenceEqual([VectorLine("_GaSnapVec", heldVectors), VectorLine("_GaSnapList", heldVectors), MatrixLine("_GaSnapMat", heldMatrices)]),
            $"the call's values were sent:\n{string.Join("\n", sent)}");

        var context = new ScriptableRenderContext();
        context.ExecuteCommandBuffer(buffer);
        buffer.Clear();
        buffer.SetGlobalVectorArray("_GaSnapVec", Vectors(1, 99f));
        buffer.Release();
        context.Submit();
        var queued = CommandWireReader.Take();
        Ensure(queued.SequenceEqual(sent), $"commands queued before Clear, a new record and Release are still sent as they were:\n{string.Join("\n", queued)}");
    }

    private static void ContextDefersArraysUntilSubmit()
    {
        var (vectors, matrices) = (Vectors(2, 4f), Matrices(1, 4f));
        var buffer = RecordEveryOverload(vectors, matrices);
        new ScriptableRenderContext().Submit();
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        var direct = CommandWireReader.Take();
        var context = new ScriptableRenderContext();
        context.ExecuteCommandBuffer(buffer);
        Ensure(ScriptableRenderContext.PendingCount == 8 && CommandWireReader.Take().Length == 0, "the context holds all eight array commands until Submit");
        context.Submit();
        Ensure(CommandWireReader.Take().SequenceEqual(direct) && direct.Length == 8, "Submit sends the same records as Graphics.ExecuteCommandBuffer");
        buffer.Release();
    }

    private static void CommandBufferRefusesBadArraysBeforeRecording()
    {
        var buffer = new CommandBuffer();
        var unregistered = int.MinValue + 17;
        var tooLong = new string('n', 257);
        var bad = new (string What, Type Error, Action Call)[]
        {
            ("a null vector array", typeof(ArgumentNullException), () => buffer.SetGlobalVectorArray("_GaBad", (Vector4[])null!)),
            ("a null matrix list", typeof(ArgumentNullException), () => buffer.SetGlobalMatrixArray("_GaBad", (List<Matrix4x4>)null!)),
            ("a null name", typeof(ArgumentNullException), () => buffer.SetGlobalVectorArray((string)null!, Vectors(1, 1f))),
            ("an empty name", typeof(ArgumentException), () => buffer.SetGlobalMatrixArray("", Matrices(1, 1f))),
            ("a name of 257 bytes", typeof(ArgumentException), () => buffer.SetGlobalVectorArray(tooLong, Vectors(1, 1f))),
            ("an unregistered id", typeof(ArgumentException), () => buffer.SetGlobalMatrixArray(unregistered, Matrices(1, 1f))),
            ("an unregistered id with vectors", typeof(ArgumentException), () => buffer.SetGlobalVectorArray(unregistered, new List<Vector4>(Vectors(1, 1f)))),
            ("an empty vector array", typeof(ArgumentException), () => buffer.SetGlobalVectorArray("_GaBad", Array.Empty<Vector4>())),
            ("an empty matrix list", typeof(ArgumentException), () => buffer.SetGlobalMatrixArray("_GaBad", new List<Matrix4x4>())),
            ("1024 vectors", typeof(ArgumentException), () => buffer.SetGlobalVectorArray("_GaBad", new Vector4[1024])),
            ("1024 matrices in a list", typeof(ArgumentException), () => buffer.SetGlobalMatrixArray("_GaBad", new List<Matrix4x4>(new Matrix4x4[1024]))),
            ("a NaN vector component", typeof(ArgumentException), () => buffer.SetGlobalVectorArray("_GaBad", new[] { new Vector4(float.NaN, 0, 0, 0) })),
            ("an infinite matrix component", typeof(ArgumentException), () => buffer.SetGlobalMatrixArray("_GaBad", new[] { WithElement(float.PositiveInfinity) })),
        };
        foreach (var (what, error, call) in bad)
        {
            var thrown = Catch(call, what);
            Ensure(thrown.GetType() == error, $"{what}: expected {error.Name}, got {thrown.GetType().Name}: {thrown.Message}");
        }
        Ensure(buffer.commandCount == 0, "no refused call recorded anything");
        buffer.SetGlobalVectorArray("_GaBad", Vectors(1023, 1f));
        buffer.SetGlobalMatrixArray(new string('n', 256), Matrices(1023, 1f));
        Ensure(buffer.commandCount == 2, "1023 elements and a 256-byte name are recorded");

        buffer.Release();
        foreach (var (what, _, call) in bad)
            Throws<ObjectDisposedException>(call, $"{what} on a released buffer", "CommandBuffer");
    }

    /// <summary>An id no name was registered for; <see cref="RefuseUnregisteredId"/> checks that it stays so.</summary>
    private const int UnregisteredId = int.MinValue + 29;

    /// <summary>The most names the world's global state holds of one array kind.</summary>
    private const int MaxNativeNames = 1024;

    /// <summary>What the world says when a list of one kind is full.</summary>
    private const string CapacityRefusal = "a set of properties holds at most 1024 values of one kind";

    /// <summary>A stored vector array and matrix array, by name, that a refusal must leave as they were.</summary>
    private sealed record Sentinels(string VectorName, Vector4[] VectorValues, string MatrixName, Matrix4x4[] MatrixValues)
    {
        public byte[] VectorBytes => VectorEntry(VectorName, VectorValues);

        public byte[] MatrixBytes => MatrixEntry(MatrixName, MatrixValues);
    }

    private static void StaticArraysRefuseAnUnregisteredId()
    {
        var (vectors, matrices) = (Vectors(2, 12f), Matrices(2, 12f));
        Shader.SetGlobalVectorArray("_GaUnregVec", vectors);
        Shader.SetGlobalMatrixArray("_GaUnregMat", matrices);
        var held = new Sentinels("_GaUnregVec", vectors, "_GaUnregMat", matrices);
        var before = NativeArrays(held);
        RefuseUnregisteredId("a loaded world");
        SentinelsAreUnchanged(held, before, "a loaded world");
    }

    /// <summary>Calls all four static array setters with valid values under an id no name was registered for. Each must throw
    /// <see cref="ArgumentException"/>, and nothing may be stored under the id, in the managed store or through its getters.</summary>
    private static void RefuseUnregisteredId(string stage)
    {
        Ensure(Shader.PropertyName(UnregisteredId) is null, $"{stage}: the id the refusals use has a registered name");
        var (vectors, matrices) = (Vectors(2, 11f), Matrices(2, 11f));
        var calls = new (string What, Action Call)[]
        {
            ("vector array", () => Shader.SetGlobalVectorArray(UnregisteredId, vectors)),
            ("vector list", () => Shader.SetGlobalVectorArray(UnregisteredId, new List<Vector4>(vectors))),
            ("matrix array", () => Shader.SetGlobalMatrixArray(UnregisteredId, matrices)),
            ("matrix list", () => Shader.SetGlobalMatrixArray(UnregisteredId, new List<Matrix4x4>(matrices))),
        };
        foreach (var (what, call) in calls)
        {
            Throws<ArgumentException>(call, $"{stage}: an unregistered id with a {what}", "no shader property has the id");
            NothingIsStoredUnderTheUnregisteredId($"{stage}, after the {what}");
        }
    }

    private static void NothingIsStoredUnderTheUnregisteredId(string stage)
    {
        Ensure(Shader.GetGlobalVectorArray(UnregisteredId) == null && Shader.GetGlobalMatrixArray(UnregisteredId) == null, $"{stage}: a refused id stores nothing");
        var (vectorList, matrixList) = (new List<Vector4> { Vector4.one }, new List<Matrix4x4> { Matrix4x4.identity });
        Shader.GetGlobalVectorArray(UnregisteredId, vectorList);
        Shader.GetGlobalMatrixArray(UnregisteredId, matrixList);
        Ensure(vectorList.Count == 0 && matrixList.Count == 0, $"{stage}: the list getters find no array under a refused id");
        Ensure(!Shader.Globals.VectorArrays.ContainsKey(UnregisteredId) && !Shader.Globals.MatrixArrays.ContainsKey(UnregisteredId), $"{stage}: the global store holds no entry under a refused id");
    }

    /// <summary>The global vector and matrix arrays of a snapshot: how many names each list holds and the bytes of both lists.</summary>
    private sealed record ArrayLists(int VectorCount, int MatrixCount, byte[] Bytes);

    private static bool SameLists(ArrayLists expected, ArrayLists actual) =>
        expected.VectorCount == actual.VectorCount && expected.MatrixCount == actual.MatrixCount && expected.Bytes.SequenceEqual(actual.Bytes);

    /// <summary>The world's global vector and matrix arrays now, read from a snapshot of a frame that has work.</summary>
    private static ArrayLists NativeArrays(Sentinels held)
    {
        Trigger();
        return GlobalArrayLists(Snapshot(), held.VectorBytes, held.MatrixBytes);
    }

    private static void SentinelsAreStored(Sentinels held, string when)
    {
        Ensure(Shader.GetGlobalVectorArray(held.VectorName) is { } vectors && vectors.SequenceEqual(held.VectorValues), $"{when}: the stored vector sentinel changed");
        Ensure(Shader.GetGlobalMatrixArray(held.MatrixName) is { } matrices && matrices.SequenceEqual(held.MatrixValues), $"{when}: the stored matrix sentinel changed");
    }

    /// <summary>The stored sentinels and the world's global arrays, names and values of both kinds, are what they were.</summary>
    private static void SentinelsAreUnchanged(Sentinels held, ArrayLists before, string when)
    {
        SentinelsAreStored(held, when);
        Ensure(SameLists(before, NativeArrays(held)), $"{when}: the world's global vector or matrix arrays changed");
    }

    /// <summary>Finds the one place in the snapshot that reads as the global state's vector array list followed by its matrix
    /// array list, which are the last two lists of its properties. The lists must hold the anchor entries, which the caller set
    /// as global arrays, so no other bytes can pass for them.</summary>
    private static ArrayLists GlobalArrayLists(byte[] snapshot, byte[] vectorAnchor, byte[] matrixAnchor)
    {
        ArrayLists? found = null;
        for (var start = 0; start < snapshot.Length; start++)
        {
            if (TryReadLists(snapshot, start, vectorAnchor, matrixAnchor) is not { } lists) continue;
            Ensure(found is null, $"the snapshot reads as global array lists at two places, one of them at byte {start}");
            found = lists;
        }
        return found ?? throw new InvalidOperationException("the snapshot holds no global array lists with the anchor entries");
    }

    private static ArrayLists? TryReadLists(byte[] bytes, int start, byte[] vectorAnchor, byte[] matrixAnchor)
    {
        var at = start;
        if (!TryReadList(bytes, ref at, 4, vectorAnchor, out var vectors) || !TryReadList(bytes, ref at, 16, matrixAnchor, out var matrices)) return null;
        return new ArrayLists(vectors, matrices, bytes[start..at]);
    }

    /// <summary>Reads a list of named arrays at <paramref name="at"/>: an entry count, then each entry's name length, name,
    /// element count and floats of <paramref name="width"/> to an element, names strictly ascending by bytes. It succeeds
    /// only for a well-formed list that holds the anchor entry.</summary>
    private static bool TryReadList(byte[] bytes, ref int at, int width, byte[] anchor, out int entries)
    {
        entries = 0;
        if (!TryReadInt(bytes, ref at, out var count) || count < 1 || count > MaxNativeNames) return false;
        var (anchored, previousAt, previousLength) = (false, 0, 0);
        for (var index = 0; index < count; index++)
        {
            var begin = at;
            if (!TryReadEntry(bytes, ref at, width, out var nameAt, out var nameLength)) return false;
            var name = new ReadOnlySpan<byte>(bytes, nameAt, nameLength);
            if (index > 0 && new ReadOnlySpan<byte>(bytes, previousAt, previousLength).SequenceCompareTo(name) >= 0) return false;
            (previousAt, previousLength) = (nameAt, nameLength);
            anchored |= new ReadOnlySpan<byte>(bytes, begin, at - begin).SequenceEqual(new ReadOnlySpan<byte>(anchor));
        }
        entries = count;
        return anchored;
    }

    private static bool TryReadEntry(byte[] bytes, ref int at, int width, out int nameAt, out int nameLength)
    {
        (nameAt, nameLength) = (0, 0);
        if (!TryReadInt(bytes, ref at, out var length) || length < 1 || length > 256 || at + length > bytes.Length) return false;
        (nameAt, nameLength) = (at, length);
        at += length;
        if (!TryReadInt(bytes, ref at, out var elements) || elements < 1 || elements > 1023) return false;
        var payload = elements * width * 4;
        if (at + payload > bytes.Length) return false;
        for (var offset = 0; offset < payload; offset += 4)
            if (!float.IsFinite(BinaryPrimitives.ReadSingleLittleEndian(bytes.AsSpan(at + offset, 4)))) return false;
        at += payload;
        return true;
    }

    private static bool TryReadInt(byte[] bytes, ref int at, out int value)
    {
        value = 0;
        if (at + 4 > bytes.Length) return false;
        value = BinaryPrimitives.ReadInt32LittleEndian(bytes.AsSpan(at, 4));
        at += 4;
        return true;
    }

    private static string FillerName(bool matrices, int index) => $"_GaFill{(matrices ? 'M' : 'V')}{index:D4}";

    /// <summary>Writes one new name of finite floats (four for a vector, sixteen for a matrix) to the world's own list of
    /// global arrays of one kind with the native binding, which leaves the managed store alone. A refusal throws.</summary>
    private static void WriteFiller(bool matrices, int index)
    {
        var bytes = Encoding.UTF8.GetBytes(FillerName(matrices, index));
        var floats = Enumerable.Range(0, matrices ? 16 : 4).Select(i => (float)(index + i)).ToArray();
        fixed (byte* name = bytes)
        fixed (float* data = floats)
            DisunityHost.ThrowIfError(matrices
                ? NativeBindings.disunity_shader_global_matrix_array_write(DisunityHost.WorldHandle, name, (uint)bytes.Length, data, 1)
                : NativeBindings.disunity_shader_global_vector_array_write(DisunityHost.WorldHandle, name, (uint)bytes.Length, data, 1));
    }

    private static bool IsCapacityRefusal(DisunityException error) =>
        error.Reason == DisunityErrorReason.InvalidArgument && error.Message.Contains(CapacityRefusal);

    /// <summary>Fills the world's own list of global arrays of one kind with new names until the world refuses one as a full
    /// list, and returns how many names it took. The world reports a full list as an invalid argument whose text names the
    /// limit, so any other error, which a loader fault or a bad name would be, is not caught.</summary>
    private static int FillNativeArrays(bool matrices)
    {
        for (var index = 0; index <= MaxNativeNames; index++)
        {
            try { WriteFiller(matrices, index); }
            catch (DisunityException error) when (IsCapacityRefusal(error)) { return index; }
        }
        throw new InvalidOperationException($"the world's list took more than {MaxNativeNames} names, so it has no limit to reach");
    }

    private static void NativeRefusalLeavesNoManagedCopy()
    {
        DisunityHost.LoadWorld();
        try
        {
            var (kept, keptMatrices) = (Vectors(2, 21f), Matrices(2, 21f));
            Shader.SetGlobalVectorArray("_GaKeptVec", kept);
            Shader.SetGlobalMatrixArray("_GaKeptMat", keptMatrices);
            var held = new Sentinels("_GaKeptVec", kept, "_GaKeptMat", keptMatrices);
            var existing = NativeArrays(held);
            var (filledVectors, filledMatrices) = (FillNativeArrays(matrices: false), FillNativeArrays(matrices: true));
            Ensure(existing.VectorCount + filledVectors == MaxNativeNames && existing.MatrixCount + filledMatrices == MaxNativeNames,
                $"the world's lists are full: {existing.VectorCount} existing + {filledVectors} new vectors and {existing.MatrixCount} existing + {filledMatrices} new matrices, each of {MaxNativeNames}");
            var full = NativeArrays(held);
            Ensure(full.VectorCount == MaxNativeNames && full.MatrixCount == MaxNativeNames, $"the snapshot holds {full.VectorCount} vector and {full.MatrixCount} matrix names, each of {MaxNativeNames}");
            Ensure(Count(full.Bytes, Encoding.UTF8.GetBytes(FillerName(matrices: false, filledVectors))) == 0 && Count(full.Bytes, Encoding.UTF8.GetBytes(FillerName(matrices: true, filledMatrices))) == 0,
                "the name the full list refused is not in the world");
            RefusedArraysPastTheLimitLeaveTheSentinels(held, full);
            Ensure(Shader.GetGlobalVectorArray("_GaOverVec") == null && Shader.GetGlobalMatrixArray("_GaOverMat") == null, "a refused array is not stored");
            Ensure(Shader.GetGlobalVectorArray("_GaKeptVec")!.SequenceEqual(kept) && Shader.GetGlobalMatrixArray("_GaKeptMat")!.SequenceEqual(keptMatrices), "an earlier vector and matrix array are as they were");

            Trigger();
            var snapshot = Snapshot();
            Ensure(Count(snapshot, Encoding.UTF8.GetBytes("_GaOverVec")) == 0 && Count(snapshot, Encoding.UTF8.GetBytes("_GaOverMat")) == 0, "the world holds neither refused name");
            Ensure(Count(snapshot, VectorEntry("_GaKeptVec", kept)) == 1 && Count(snapshot, MatrixEntry("_GaKeptMat", keptMatrices)) == 1, "the world still holds the earlier vectors and matrices");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    /// <summary>Calls every static array setter with valid values under a new name or id while the world's list of that kind is
    /// full. Each must throw the capacity error, and after each the stored sentinels and the world's global arrays, matrices
    /// included, are what they were.</summary>
    private static void RefusedArraysPastTheLimitLeaveTheSentinels(Sentinels held, ArrayLists full)
    {
        var (vectors, matrices) = (Vectors(3, 31f), Matrices(3, 31f));
        var refused = new (string What, Action Call)[]
        {
            ("vector array", () => Shader.SetGlobalVectorArray("_GaOverVec", vectors)),
            ("vector list by id", () => Shader.SetGlobalVectorArray(Shader.PropertyToID("_GaOverVec"), new List<Vector4>(vectors))),
            ("matrix array", () => Shader.SetGlobalMatrixArray("_GaOverMat", matrices)),
            ("matrix list by id", () => Shader.SetGlobalMatrixArray(Shader.PropertyToID("_GaOverMat"), new List<Matrix4x4>(matrices))),
        };
        foreach (var (what, call) in refused)
        {
            Throws<DisunityException>(call, $"a {what} past the world's limit", "InvalidArgument", CapacityRefusal);
            Ensure(Shader.GetGlobalVectorArray("_GaOverVec") == null && Shader.GetGlobalMatrixArray("_GaOverMat") == null, $"a refused {what} is not stored");
            SentinelsAreUnchanged(held, full, $"after a refused {what}");
        }
    }

    private static void ArraysSetBeforeLoadAreSentWhenTheWorldLoads()
    {
        var (vectors, matrices) = (Vectors(2, 8f), Matrices(2, 8f));
        Shader.SetGlobalVectorArray("_GaPreVec", vectors);
        Shader.SetGlobalMatrixArray("_GaPreMat", new List<Matrix4x4>(matrices));
        var held = new Sentinels("_GaPreVec", vectors, "_GaPreMat", matrices);
        var material = new Material(Material.SpriteLit);
        material.SetVectorArray("_GaMaterialVec", Vectors(1, 1f));
        ArrayLists? firstLoad = null;
        for (var load = 0; load < 3; load++)
        {
            if (load > 0)
            {
                RefuseUnregisteredId($"before load {load}, with no world loaded");
                SentinelsAreStored(held, $"before load {load}, after the refusals with no world loaded");
            }
            DisunityHost.LoadWorld();
            try
            {
                Trigger();
                var snapshot = Snapshot();
                Ensure(Count(snapshot, VectorEntry("_GaPreVec", vectors)) == 1, $"load {load}: the vectors set before the load are in the global state");
                Ensure(Count(snapshot, MatrixEntry("_GaPreMat", matrices)) == 1, $"load {load}: the matrices set before the load are in the global state");
                Ensure(Count(snapshot, VectorEntry("_GaMaterialVec", Vectors(1, 1f))) == 0, $"load {load}: a material's array is not sent");
                var lists = GlobalArrayLists(snapshot, held.VectorBytes, held.MatrixBytes);
                firstLoad ??= lists;
                Ensure(SameLists(firstLoad, lists), $"load {load}: the global arrays are those of the first load, so a refused id sent nothing");
                NothingIsStoredUnderTheUnregisteredId($"load {load}");
                RefuseUnregisteredId($"load {load}, with a world loaded");
                SentinelsAreUnchanged(held, lists, $"load {load}, after the refusals with a world loaded");
            }
            finally
            {
                DisunityHost.UnloadWorld();
            }
        }
    }
}
