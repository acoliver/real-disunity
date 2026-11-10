using System.Globalization;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>CommandBuffer.SetupCameraProperties records the camera's view and projection when it is called, the global array
/// members bind a copy of their values when the buffer runs, and the DrawProcedural overloads name themselves.</summary>
internal static class CommandBufferCameraTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            CameraSetupRecordsViewAndProjectionAsTheyAreNow();
            Console.WriteLine("PASS: CommandBuffer.SetupCameraProperties records the camera's view and projection as they are when it is called");
            CameraSetupRefusesWhatItCannotRecord();
            Console.WriteLine("PASS: CommandBuffer.SetupCameraProperties refuses a null or destroyed camera and a released buffer");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
        VectorArraysBindWhenTheBufferRuns();
        Console.WriteLine("PASS: SetGlobalVectorArray binds a copy of the values when the buffer runs, by id and by name, from an array or a list");
        FloatAndMatrixArraysBindWhenTheBufferRuns();
        Console.WriteLine("PASS: SetGlobalFloatArray and SetGlobalMatrixArray bind a copy of the values when the buffer runs");
        GlobalArraysRefuseWhatTheyCannotRecord();
        Console.WriteLine("PASS: the global array members refuse a null or empty array and a released buffer, and record nothing");
        DrawProceduralNamesItself();
        Console.WriteLine("PASS: CommandBuffer.DrawProcedural throws NotSupportedException naming itself and records nothing");
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

    private static void VectorArraysBindWhenTheBufferRuns()
    {
        int id = Shader.PropertyToID("_CommandBufferTestVectors"), byName = Shader.PropertyToID("_CommandBufferTestVectorsByName");
        int byList = Shader.PropertyToID("_CommandBufferTestVectorsByList"), byNameList = Shader.PropertyToID("_CommandBufferTestVectorsByNameList");
        var values = new[] { new Vector4(1, 2, 3, 4), new Vector4(5, 6, 7, 8) };
        var buffer = new CommandBuffer();
        buffer.SetGlobalVectorArray(id, values);
        buffer.SetGlobalVectorArray("_CommandBufferTestVectorsByName", values);
        buffer.SetGlobalVectorArray(byList, new List<Vector4>(values));
        buffer.SetGlobalVectorArray("_CommandBufferTestVectorsByNameList", new List<Vector4>(values));
        Ensure(buffer.commandCount == 4, "each call is one command");
        Ensure(new[] { id, byName, byList, byNameList }.All(key => !Shader.Globals.VectorArrays.ContainsKey(key)), "recording binds nothing yet");
        values[0] = new Vector4(-1, -1, -1, -1);
        values[1] = Vector4.zero;
        foreach (var send in buffer.Commands()) send();
        foreach (var key in new[] { id, byName, byList, byNameList })
            Ensure(Shader.GetGlobalVectorArray(key)!.SequenceEqual(new[] { new Vector4(1, 2, 3, 4), new Vector4(5, 6, 7, 8) }),
                $"the values from when the call was made are bound when the buffer runs (property {key})");
        buffer.SetGlobalVectorArray(id, new[] { new Vector4(9, 9, 9, 9) });
        foreach (var send in buffer.Commands()[4..]) send();
        Ensure(Shader.GetGlobalVectorArray(id)!.SequenceEqual(new[] { new Vector4(9, 9, 9, 9) }), "a later binding replaces the earlier one");
        foreach (var key in new[] { id, byName, byList, byNameList }) Shader.Globals.VectorArrays.Remove(key);
    }

    private static void FloatAndMatrixArraysBindWhenTheBufferRuns()
    {
        string[] floatNames = ["_CbFloatsId", "_CbFloatsName", "_CbFloatsIdList", "_CbFloatsNameList"];
        string[] matrixNames = ["_CbMatricesId", "_CbMatricesName", "_CbMatricesIdList", "_CbMatricesNameList"];
        var floatValues = new[] { 1f, 2f, 3f };
        var matrixValues = new[] { Matrix4x4.identity, Matrix4x4.Translate(new Vector3(1, 2, 3)) };
        var buffer = new CommandBuffer();
        buffer.SetGlobalFloatArray(Shader.PropertyToID(floatNames[0]), floatValues);
        buffer.SetGlobalFloatArray(floatNames[1], floatValues);
        buffer.SetGlobalFloatArray(Shader.PropertyToID(floatNames[2]), new List<float>(floatValues));
        buffer.SetGlobalFloatArray(floatNames[3], new List<float>(floatValues));
        buffer.SetGlobalMatrixArray(Shader.PropertyToID(matrixNames[0]), matrixValues);
        buffer.SetGlobalMatrixArray(matrixNames[1], matrixValues);
        buffer.SetGlobalMatrixArray(Shader.PropertyToID(matrixNames[2]), new List<Matrix4x4>(matrixValues));
        buffer.SetGlobalMatrixArray(matrixNames[3], new List<Matrix4x4>(matrixValues));
        Ensure(buffer.commandCount == 8, "each call is one command");
        Ensure(floatNames.All(name => Shader.GetGlobalFloatArray(name) == null) && matrixNames.All(name => Shader.GetGlobalMatrixArray(name) == null),
            "recording binds nothing yet");
        floatValues[0] = 100f;
        matrixValues[1] = Matrix4x4.zero;
        foreach (var send in buffer.Commands()) send();
        var expected = new[] { Matrix4x4.identity, Matrix4x4.Translate(new Vector3(1, 2, 3)) };
        foreach (var name in floatNames)
            Ensure(Shader.GetGlobalFloatArray(name)!.SequenceEqual(new[] { 1f, 2f, 3f }), $"the float values from when the call was made are bound ({name})");
        foreach (var name in matrixNames)
            Ensure(Shader.GetGlobalMatrixArray(name)!.SequenceEqual(expected), $"the matrix values from when the call was made are bound ({name})");
        foreach (var name in floatNames) Shader.Globals.FloatArrays.Remove(Shader.PropertyToID(name));
        foreach (var name in matrixNames) Shader.Globals.MatrixArrays.Remove(Shader.PropertyToID(name));
    }

    private static void GlobalArraysRefuseWhatTheyCannotRecord()
    {
        var buffer = new CommandBuffer();
        Throws<ArgumentNullException>(() => buffer.SetGlobalVectorArray(1, (Vector4[])null!), "a null vector array", "values");
        Throws<ArgumentNullException>(() => buffer.SetGlobalVectorArray("_X", (List<Vector4>)null!), "a null vector list", "values");
        Throws<ArgumentNullException>(() => buffer.SetGlobalFloatArray(1, (float[])null!), "a null float array", "values");
        Throws<ArgumentNullException>(() => buffer.SetGlobalFloatArray("_X", (List<float>)null!), "a null float list", "values");
        Throws<ArgumentNullException>(() => buffer.SetGlobalMatrixArray(1, (Matrix4x4[])null!), "a null matrix array", "values");
        Throws<ArgumentNullException>(() => buffer.SetGlobalMatrixArray("_X", (List<Matrix4x4>)null!), "a null matrix list", "values");
        Throws<ArgumentException>(() => buffer.SetGlobalVectorArray(1, new Vector4[0]), "an empty array", "at least one element");
        Ensure(buffer.commandCount == 0, "a refused array records nothing");
        buffer.Release();
        Throws<ObjectDisposedException>(() => buffer.SetGlobalVectorArray(1, new[] { Vector4.one }), "a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.SetGlobalFloatArray("_X", new[] { 1f }), "a released buffer", "CommandBuffer");
        Throws<ObjectDisposedException>(() => buffer.SetGlobalMatrixArray("_X", new List<Matrix4x4> { Matrix4x4.identity }), "a released buffer", "CommandBuffer");
    }

    private static void DrawProceduralNamesItself()
    {
        var buffer = new CommandBuffer();
        var material = new Material(Material.SpriteLit);
        var properties = new MaterialPropertyBlock();
        using var indices = new GraphicsBuffer(GraphicsBuffer.Target.Index, 3, 4);
        var calls = new Action[]
        {
            () => buffer.DrawProcedural(Matrix4x4.identity, material, 0, MeshTopology.Triangles, 3, 1, properties),
            () => buffer.DrawProcedural(Matrix4x4.identity, material, 0, MeshTopology.Triangles, 3, 1),
            () => buffer.DrawProcedural(Matrix4x4.identity, material, 0, MeshTopology.Triangles, 3),
            () => buffer.DrawProcedural(indices, Matrix4x4.identity, material, 0, MeshTopology.Triangles, 3, 1, properties),
            () => buffer.DrawProcedural(indices, Matrix4x4.identity, material, 0, MeshTopology.Triangles, 3, 1),
            () => buffer.DrawProcedural(indices, Matrix4x4.identity, material, 0, MeshTopology.Triangles, 3),
        };
        foreach (var call in calls) Throws<NotSupportedException>(call, "DrawProcedural", "CommandBuffer.DrawProcedural", "cannot record this member yet");
        Ensure(buffer.commandCount == 0, "a procedural draw records nothing");
        buffer.Release();
        Throws<ObjectDisposedException>(calls[0], "a released buffer", "CommandBuffer");
    }
}
