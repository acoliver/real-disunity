using System.Globalization;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>CommandBuffer.SetupCameraProperties records the camera's view and projection when it is called, and the global float array
/// and the indexed DrawProcedural members name themselves and record nothing, through a real world and the public execution paths.</summary>
internal static class CommandBufferCameraTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            CameraSetupRecordsViewAndProjectionAsTheyAreNow();
            Console.WriteLine("PASS: CommandBuffer.SetupCameraProperties records the camera's view and projection as they are when it is called");
            CameraSetupLeavesTheBoundTargetAndDefersToSubmit();
            Console.WriteLine("PASS: CommandBuffer.SetupCameraProperties keeps the bound target, ignores the camera's own, and the context sends it at Submit");
            CameraSetupRefusesWhatItCannotRecord();
            Console.WriteLine("PASS: CommandBuffer.SetupCameraProperties refuses a null or destroyed camera and a released buffer");
            GlobalArraysNameThemselvesAndRecordNothing();
            Console.WriteLine("PASS: every SetGlobalFloatArray overload throws NotSupportedException naming itself, records nothing and changes no global or world state");
            GlobalArraysRefuseAReleasedBufferFirst();
            Console.WriteLine("PASS: a released buffer throws ObjectDisposedException from every global array overload, including a null or empty array");
            IndexedDrawProceduralNamesItself();
            Console.WriteLine("PASS: CommandBuffer.DrawProcedural with an index buffer throws NotSupportedException naming itself and records nothing");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static Camera NewCamera()
    {
        var camera = DisunityHost.CreateGameObject().AddComponent<Camera>();
        camera.orthographic = false;
        camera.fieldOfView = 50f;
        camera.transform.position = new Vector3(2f, -1f, 4f);
        return camera;
    }

    private static float[] Columns(Matrix4x4 m)
    {
        var values = new float[16];
        for (var column = 0; column < 4; column++)
            for (var row = 0; row < 4; row++) values[column * 4 + row] = m[row, column];
        return values;
    }

    private static float[] Recorded(string record, string key)
    {
        var start = record.IndexOf(key + "=(", StringComparison.Ordinal) + key.Length + 2;
        var end = record.IndexOf(')', start);
        return record[start..end].Split(' ').Select(text => float.Parse(text, CultureInfo.InvariantCulture)).ToArray();
    }

    private static void CameraSetupRecordsViewAndProjectionAsTheyAreNow()
    {
        var camera = NewCamera();
        var view = camera.worldToCameraMatrix;
        var projection = camera.projectionMatrix;
        Ensure(!Columns(view).SequenceEqual(Columns(projection)), "the view and the projection differ, so a swap would show");
        CommandWireReader.Take();
        var buffer = new CommandBuffer();
        buffer.SetupCameraProperties(camera);
        camera.transform.position = new Vector3(9f, 9f, 9f);
        camera.fieldOfView = 80f;
        Ensure(buffer.commandCount == 1, "one command is recorded");
        Ensure(CommandWireReader.Take().Length == 0, "nothing reaches the world before the buffer runs");
        Graphics.ExecuteCommandBuffer(buffer);
        var taken = CommandWireReader.Take();
        Ensure(taken.Length == 1 && taken[0].StartsWith("ViewProjection ", StringComparison.Ordinal), $"one view and projection record: {string.Join(" | ", taken)}");
        Ensure(Recorded(taken[0], "view").SequenceEqual(Columns(view)), $"the view as it was when called: {taken[0]}");
        Ensure(Recorded(taken[0], "projection").SequenceEqual(Columns(projection)), $"the projection as it was when called: {taken[0]}");
        Ensure(!Columns(camera.worldToCameraMatrix).SequenceEqual(Columns(view)), "the camera did move after the call");
        buffer.Release();
    }

    private static void CameraSetupRefusesWhatItCannotRecord()
    {
        var buffer = new CommandBuffer();
        Throws<ArgumentNullException>(() => buffer.SetupCameraProperties(null!), "a null camera", "camera");
        var destroyed = NewCamera();
        UnityEngine.Object.DestroyImmediate(destroyed);
        Throws<InvalidOperationException>(() => buffer.SetupCameraProperties(destroyed), "a destroyed camera");
        Ensure(buffer.commandCount == 0, "a refused setup records nothing");
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.SetupCameraProperties(NewCamera()), "a released buffer", "CommandBuffer");
    }

    private static void CameraSetupLeavesTheBoundTargetAndDefersToSubmit()
    {
        var camera = NewCamera();
        var (cameraTarget, bound) = (NewTarget(), NewTarget());
        camera.targetTexture = cameraTarget;
        Ensure(cameraTarget.WorldId != bound.WorldId, "the camera's target differs from the bound target");
        var buffer = new CommandBuffer();
        buffer.SetRenderTarget(bound);
        buffer.SetupCameraProperties(camera);
        Ensure(buffer.commandCount == 2, "the target and the setup are two commands");
        new ScriptableRenderContext().Submit();
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        var direct = CommandWireReader.Take();
        Ensure(direct.Length == 2, $"two records: {string.Join(" | ", direct)}");
        Ensure(direct[0] == $"SetRenderTarget colors=[Render:{bound.WorldId}] depth=- mip=0 face=-1 slice=0", $"the bound target: {direct[0]}");
        Ensure(direct[1].StartsWith("ViewProjection ", StringComparison.Ordinal), $"the camera matrices only: {direct[1]}");
        Ensure(!direct.Any(record => record.StartsWith("SetupCamera", StringComparison.Ordinal) || record.Contains($"Render:{cameraTarget.WorldId}")),
            "the camera's own target is never bound");
        var context = new ScriptableRenderContext();
        context.ExecuteCommandBuffer(buffer);
        Ensure(ScriptableRenderContext.PendingCount == 2 && CommandWireReader.Take().Length == 0, "the context holds both commands until Submit");
        context.Submit();
        Ensure(CommandWireReader.Take().SequenceEqual(direct), "Submit sends the same two records as Graphics.ExecuteCommandBuffer");
        buffer.Release();
    }

    private static RenderTexture NewTarget()
    {
        var texture = new RenderTexture(16, 16, 0, RenderTextureFormat.ARGB32);
        texture.Create();
        return texture;
    }

    private const string FloatsId = "_CbFloatsId", VectorsId = "_CbVectorsId", MatricesId = "_CbMatricesId";

    /// <summary>The twelve overloads, with the values chosen by <paramref name="mode"/>: "values", "null" or "empty".</summary>
    private static (string Member, Action Call)[] ArrayCalls(CommandBuffer buffer, string mode)
    {
        var (floats, vectors, matrices) = (Pick(mode, new[] { 1f, 2f }), Pick(mode, new[] { Vector4.one, Vector4.zero }),
            Pick(mode, new[] { Matrix4x4.identity, Matrix4x4.zero }));
        var (floatList, vectorList, matrixList) = (PickList(mode, floats), PickList(mode, vectors), PickList(mode, matrices));
        var (floatId, vectorId, matrixId) = (Shader.PropertyToID(FloatsId), Shader.PropertyToID(VectorsId), Shader.PropertyToID(MatricesId));
        return
        [
            (nameof(CommandBuffer.SetGlobalFloatArray), () => buffer.SetGlobalFloatArray(FloatsId, floats!)),
            (nameof(CommandBuffer.SetGlobalFloatArray), () => buffer.SetGlobalFloatArray(FloatsId, floatList!)),
            (nameof(CommandBuffer.SetGlobalFloatArray), () => buffer.SetGlobalFloatArray(floatId, floatList!)),
            (nameof(CommandBuffer.SetGlobalFloatArray), () => buffer.SetGlobalFloatArray(floatId, floats!)),
            (nameof(CommandBuffer.SetGlobalVectorArray), () => buffer.SetGlobalVectorArray(VectorsId, vectors!)),
            (nameof(CommandBuffer.SetGlobalVectorArray), () => buffer.SetGlobalVectorArray(VectorsId, vectorList!)),
            (nameof(CommandBuffer.SetGlobalVectorArray), () => buffer.SetGlobalVectorArray(vectorId, vectorList!)),
            (nameof(CommandBuffer.SetGlobalVectorArray), () => buffer.SetGlobalVectorArray(vectorId, vectors!)),
            (nameof(CommandBuffer.SetGlobalMatrixArray), () => buffer.SetGlobalMatrixArray(MatricesId, matrices!)),
            (nameof(CommandBuffer.SetGlobalMatrixArray), () => buffer.SetGlobalMatrixArray(MatricesId, matrixList!)),
            (nameof(CommandBuffer.SetGlobalMatrixArray), () => buffer.SetGlobalMatrixArray(matrixId, matrixList!)),
            (nameof(CommandBuffer.SetGlobalMatrixArray), () => buffer.SetGlobalMatrixArray(matrixId, matrices!)),
        ];
    }

    private static T[]? Pick<T>(string mode, T[] values) => mode switch { "null" => null, "empty" => [], _ => values };

    private static List<T>? PickList<T>(string mode, T[]? values) => mode == "null" ? null : new List<T>(values ?? []);

    private static void GlobalArraysNameThemselvesAndRecordNothing()
    {
        var (floatId, vectorId, matrixId) = (Shader.PropertyToID(FloatsId), Shader.PropertyToID(VectorsId), Shader.PropertyToID(MatricesId));
        var (vectors, matrices) = (new[] { new Vector4(7, 7, 7, 7) }, new[] { Matrix4x4.Translate(new Vector3(7, 7, 7)) });
        Shader.SetGlobalVectorArray(vectorId, vectors);
        Shader.SetGlobalMatrixArray(matrixId, matrices);
        var buffer = new CommandBuffer();
        new ScriptableRenderContext().Submit();
        CommandWireReader.Take();
        foreach (var mode in new[] { "values", "null", "empty" })
            foreach (var (member, call) in ArrayCalls(buffer, mode).Where(entry => entry.Member == nameof(CommandBuffer.SetGlobalFloatArray)))
                Throws<NotSupportedException>(call, $"{member} with {mode}", $"CommandBuffer.{member}", "cannot record this member yet");
        Ensure(buffer.commandCount == 0, "a refused float array records nothing");
        Ensure(Shader.GetGlobalVectorArray(vectorId)!.SequenceEqual(vectors)
            && Shader.GetGlobalMatrixArray(matrixId)!.SequenceEqual(matrices), "the shaders' global arrays are unchanged");
        Graphics.ExecuteCommandBuffer(buffer);
        Ensure(CommandWireReader.Take().Length == 0, "executing the buffer sends the world nothing");
        var context = new ScriptableRenderContext();
        context.ExecuteCommandBuffer(buffer);
        context.Submit();
        Ensure(ScriptableRenderContext.PendingCount == 0 && CommandWireReader.Take().Length == 0, "submitting a context sends the world nothing");
        Shader.Globals.VectorArrays.Remove(vectorId);
        Shader.Globals.MatrixArrays.Remove(matrixId);
        buffer.Release();
    }

    private static void GlobalArraysRefuseAReleasedBufferFirst()
    {
        var buffer = new CommandBuffer();
        buffer.Release();
        foreach (var mode in new[] { "values", "null", "empty" })
            foreach (var (member, call) in ArrayCalls(buffer, mode))
                Throws<ObjectDisposedException>(call, $"{member} with {mode} on a released buffer", "CommandBuffer");
    }

    private static void IndexedDrawProceduralNamesItself()
    {
        var buffer = new CommandBuffer();
        var material = new Material(Material.SpriteLit);
        var properties = new MaterialPropertyBlock();
        using var indices = new GraphicsBuffer(GraphicsBuffer.Target.Index, 3, 4);
        var calls = new Action[]
        {
            () => buffer.DrawProcedural(indices, Matrix4x4.identity, material, 0, MeshTopology.Triangles, 3, 1, properties),
            () => buffer.DrawProcedural(indices, Matrix4x4.identity, material, 0, MeshTopology.Triangles, 3, 1),
            () => buffer.DrawProcedural(indices, Matrix4x4.identity, material, 0, MeshTopology.Triangles, 3),
        };
        foreach (var call in calls) Throws<NotSupportedException>(call, "DrawProcedural with an index buffer", "CommandBuffer.DrawProcedural with an index buffer", "cannot record this member yet");
        Ensure(buffer.commandCount == 0, "an indexed procedural draw records nothing");
        Graphics.ExecuteCommandBuffer(buffer);
        Ensure(CommandWireReader.Take().Length == 0, "executing the buffer sends the world nothing");
        buffer.Release();
        Throws<ObjectDisposedException>(calls[0], "a released buffer", "CommandBuffer");
    }
}
