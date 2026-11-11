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
        Console.WriteLine("PASS: a native refusal of a valid static array (the world's list of that kind is full) throws and leaves the stored arrays and the world as they were");
        ArraysSetBeforeLoadAreSentWhenTheWorldLoads();
        Console.WriteLine("PASS: arrays set while no world is loaded are sent to the global state at load and again at reload, and materials send none");
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

    private static void StaticArraysRefuseAnUnregisteredId()
    {
        var unregistered = int.MinValue + 29;
        var (vectors, matrices) = (Vectors(2, 11f), Matrices(2, 11f));
        var calls = new (string What, Action Call)[]
        {
            ("vector array", () => Shader.SetGlobalVectorArray(unregistered, vectors)),
            ("vector list", () => Shader.SetGlobalVectorArray(unregistered, new List<Vector4>(vectors))),
            ("matrix array", () => Shader.SetGlobalMatrixArray(unregistered, matrices)),
            ("matrix list", () => Shader.SetGlobalMatrixArray(unregistered, new List<Matrix4x4>(matrices))),
        };
        foreach (var (what, call) in calls)
            Throws<ArgumentException>(call, $"an unregistered id with a {what}", "no shader property has the id");
        Ensure(Shader.GetGlobalVectorArray(unregistered) == null && Shader.GetGlobalMatrixArray(unregistered) == null, "a refused id stores nothing");
    }

    /// <summary>Fills the world's own list of global arrays of one kind to its limit with the native bindings, which leave the
    /// managed store alone, and returns how many names the list took.</summary>
    private static int FillNativeArrays(bool matrices)
    {
        var floats = new float[16];
        var taken = 0;
        while (taken < 4096)
        {
            var bytes = Encoding.UTF8.GetBytes($"_GaFill{(matrices ? 'M' : 'V')}{taken:D4}");
            DisunityResult result;
            fixed (byte* name = bytes)
            fixed (float* data = floats)
                result = matrices
                    ? NativeBindings.disunity_shader_global_matrix_array_write(DisunityHost.WorldHandle, name, (uint)bytes.Length, data, 1)
                    : NativeBindings.disunity_shader_global_vector_array_write(DisunityHost.WorldHandle, name, (uint)bytes.Length, data, 1);
            if (result.code != 0) return taken;
            taken++;
        }
        throw new InvalidOperationException("the world's list took 4096 names, so it has no limit to reach");
    }

    private static void NativeRefusalLeavesNoManagedCopy()
    {
        DisunityHost.LoadWorld();
        try
        {
            var kept = Vectors(2, 21f);
            Shader.SetGlobalVectorArray("_GaKeptVec", kept);
            Shader.SetGlobalMatrixArray("_GaKeptMat", Matrices(2, 21f));
            var filledVectors = FillNativeArrays(matrices: false);
            var filledMatrices = FillNativeArrays(matrices: true);
            Ensure(filledVectors is >= 1000 and <= 1024 && filledMatrices is >= 1000 and <= 1024, $"the world's lists are full: {filledVectors} vectors, {filledMatrices} matrices");

            var (vectors, matrices) = (Vectors(3, 31f), Matrices(3, 31f));
            var refused = new (string What, Action Call)[]
            {
                ("vector array", () => Shader.SetGlobalVectorArray("_GaOverVec", vectors)),
                ("vector list by id", () => Shader.SetGlobalVectorArray(Shader.PropertyToID("_GaOverVec"), new List<Vector4>(vectors))),
                ("matrix array", () => Shader.SetGlobalMatrixArray("_GaOverMat", matrices)),
                ("matrix list by id", () => Shader.SetGlobalMatrixArray(Shader.PropertyToID("_GaOverMat"), new List<Matrix4x4>(matrices))),
            };
            foreach (var (what, call) in refused)
                Throws<DisunityException>(call, $"a {what} past the world's limit");
            Ensure(Shader.GetGlobalVectorArray("_GaOverVec") == null && Shader.GetGlobalMatrixArray("_GaOverMat") == null, "a refused array is not stored");
            Ensure(Shader.GetGlobalVectorArray("_GaKeptVec")!.SequenceEqual(kept), "an earlier array is as it was");

            Trigger();
            var snapshot = Snapshot();
            Ensure(Count(snapshot, Encoding.UTF8.GetBytes("_GaOverVec")) == 0 && Count(snapshot, Encoding.UTF8.GetBytes("_GaOverMat")) == 0, "the world holds neither refused name");
            Ensure(Count(snapshot, VectorEntry("_GaKeptVec", kept)) == 1, "the world still holds the earlier array");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static void ArraysSetBeforeLoadAreSentWhenTheWorldLoads()
    {
        var (vectors, matrices) = (Vectors(2, 8f), Matrices(2, 8f));
        Shader.SetGlobalVectorArray("_GaPreVec", vectors);
        Shader.SetGlobalMatrixArray("_GaPreMat", new List<Matrix4x4>(matrices));
        var material = new Material(Material.SpriteLit);
        material.SetVectorArray("_GaMaterialVec", Vectors(1, 1f));
        for (var load = 0; load < 2; load++)
        {
            DisunityHost.LoadWorld();
            try
            {
                Trigger();
                var snapshot = Snapshot();
                Ensure(Count(snapshot, VectorEntry("_GaPreVec", vectors)) == 1, $"load {load}: the vectors set before the load are in the global state");
                Ensure(Count(snapshot, MatrixEntry("_GaPreMat", matrices)) == 1, $"load {load}: the matrices set before the load are in the global state");
                Ensure(Count(snapshot, VectorEntry("_GaMaterialVec", Vectors(1, 1f))) == 0, $"load {load}: a material's array is not sent");
            }
            finally
            {
                DisunityHost.UnloadWorld();
            }
        }
    }
}
