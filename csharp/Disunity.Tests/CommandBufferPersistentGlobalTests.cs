using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>A global a command buffer sets stays set once the buffer has run, as a global set with <c>Shader.SetGlobal*</c> does:
/// the managed global store holds it, the world's global shader state holds it, and a later read of either, in a later take of
/// the frame's work, still finds it. A temporary render texture has no identity that outlives its take, so a global set to one
/// stays recorded work only.</summary>
internal static unsafe class CommandBufferPersistentGlobalTests
{
    private const uint Global = 1, GlobalLink = 1;
    private const uint HasFloat = 1, HasInt = 2, HasVector = 4, HasMatrix = 8, HasTexture = 16;
    private const uint KindAsset = 1, KindRender = 2;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ATextureGlobalOutlivesTheBufferAndItsTake();
            Console.WriteLine("PASS: SetGlobalTexture of a render texture, a Texture2D and none persists in the managed store and the world's global state after the buffer runs");
            ALayeredTextureGlobalOutlivesTheBuffer();
            Console.WriteLine("PASS: SetGlobalTexture of a Cubemap and a Texture2DArray persists in the managed store and the world's global state as the asset the host draws");
            LayeredTexturesAreBlitAndCopySources();
            Console.WriteLine("PASS: a Cubemap and a Texture2DArray named by an identifier are the source of a Blit and a CopyTexture, by the asset the host draws");
            ALayeredTextureIsNeverARenderTarget();
            Console.WriteLine("PASS: a Cubemap and a Texture2DArray as a render target throw NotSupportedException naming the render target");
            ScalarGlobalsPersist();
            Console.WriteLine("PASS: SetGlobalFloat, Int, Integer, Vector, Color and Matrix, by name and id, persist as the matching Shader.SetGlobal calls do");
            KeywordsPersist();
            Console.WriteLine("PASS: EnableShaderKeyword, DisableShaderKeyword, EnableKeyword, DisableKeyword and SetKeyword persist in Shader and in the world");
            ArraysPersist();
            Console.WriteLine("PASS: SetGlobalVectorArray and SetGlobalMatrixArray persist in the managed store and the world's global state");
            ATemporaryGlobalStaysRecordedWork();
            Console.WriteLine("PASS: a global set to a temporary render texture is ordered work only and never enters the global state");
            ValuesStillApplyInOrderWithOtherRecords();
            Console.WriteLine("PASS: a global set between two records reaches the work in front of the record that follows it, with the value it had then");
            ANonExecutedBufferChangesNothing();
            Console.WriteLine("PASS: recording and queueing in a context change no global until the buffer is executed or the context submits");
            ADestroyedTextureThrowsWhenTheBufferRuns();
            Console.WriteLine("PASS: a render texture destroyed after recording throws when the buffer runs and sets nothing");
        }
        finally
        {
            ForgetTextureGlobals();
            DisunityHost.UnloadWorld();
        }
    }

    /// <summary>Clears the texture globals these tests set. Globals outlive a world, and a texture the next world cannot
    /// receive (a render texture that unloading destroyed) would be synced into it and noted as unsent there.</summary>
    private static void ForgetTextureGlobals()
    {
        foreach (var name in new[] { "_GrabT", "_GrabById", "_GrabPlain", "_PgLazyT", "_PgTemp", "_PgGone", "_GrabCube", "_GrabArray" })
            Shader.SetGlobalTexture(name, (Texture?)null);
    }

    private static DisunityPropertyRead Read(string name) => WorldGraphicsTests.Read(Global, GlobalLink, name);

    private static RenderTexture Target()
    {
        var texture = new RenderTexture(16, 16, 0, RenderTextureFormat.ARGB32);
        texture.Create();
        return texture;
    }

    /// <summary>Runs the buffer and drains the work, the way the end of the frame that ran it does.</summary>
    private static void RunAndLetTheTakeEnd(CommandBuffer buffer)
    {
        Graphics.ExecuteCommandBuffer(buffer);
        CommandWireReader.Take();
    }

    private static void ATextureGlobalOutlivesTheBufferAndItsTake()
    {
        var (target, plain) = (Target(), new Texture2D(4, 4));
        var buffer = new CommandBuffer();
        buffer.SetGlobalTexture("_GrabT", new RenderTargetIdentifier(target));
        buffer.SetGlobalTexture(Shader.PropertyToID("_GrabById"), new RenderTargetIdentifier(target));
        buffer.SetGlobalTexture("_GrabPlain", new RenderTargetIdentifier(plain));
        CommandWireReader.Take();
        Expect.That(Shader.GetGlobalTexture("_GrabT") is null, "nothing is set before the buffer runs");
        RunAndLetTheTakeEnd(buffer);
        Expect.That(ReferenceEquals(Shader.GetGlobalTexture("_GrabT"), target), "Shader.GetGlobalTexture reads the render texture a buffer set");
        Expect.That(ReferenceEquals(Shader.GetGlobalTexture("_GrabById"), target), "the id overload sets the same global store");
        Expect.That(ReferenceEquals(Shader.GetGlobalTexture("_GrabPlain"), plain), "a Texture2D named by an identifier is the managed global");
        var read = Read("_GrabT");
        Expect.That((read.present & HasTexture) != 0 && read.texture_kind == KindRender && read.texture == target.WorldId,
            $"the world's global state holds the render texture after the take ended: present={read.present} kind={read.texture_kind} texture={read.texture}");
        var plainRead = Read("_GrabPlain");
        Expect.That(plainRead.texture_kind == KindAsset && plainRead.texture == plain.RendererIdentity, "the world holds the Texture2D as the asset the host draws");
        var cleared = new CommandBuffer();
        cleared.SetGlobalTexture("_GrabT", BuiltinRenderTextureType.None);
        RunAndLetTheTakeEnd(cleared);
        Expect.That(Shader.GetGlobalTexture("_GrabT") is null && Read("_GrabT").texture_kind == 0, "setting none clears the managed and the world global");
    }

    private static void ALayeredTextureGlobalOutlivesTheBuffer()
    {
        var (cube, array) = (new Cubemap(2, TextureFormat.RGBA32, false), new Texture2DArray(2, 2, 3, TextureFormat.RGBA32, false));
        var buffer = new CommandBuffer();
        buffer.SetGlobalTexture("_GrabCube", new RenderTargetIdentifier(cube));
        buffer.SetGlobalTexture(Shader.PropertyToID("_GrabArray"), new RenderTargetIdentifier(array));
        RunAndLetTheTakeEnd(buffer);
        Expect.That(ReferenceEquals(Shader.GetGlobalTexture("_GrabCube"), cube), "Shader.GetGlobalTexture reads the Cubemap a buffer set");
        Expect.That(ReferenceEquals(Shader.GetGlobalTexture("_GrabArray"), array), "Shader.GetGlobalTexture reads the Texture2DArray a buffer set by id");
        var (cubeRead, arrayRead) = (Read("_GrabCube"), Read("_GrabArray"));
        Expect.That((cubeRead.present & HasTexture) != 0 && cubeRead.texture_kind == KindAsset && cubeRead.texture == cube.RendererIdentity,
            $"the world holds the Cubemap as the asset the host draws: kind={cubeRead.texture_kind} texture={cubeRead.texture}");
        Expect.That((arrayRead.present & HasTexture) != 0 && arrayRead.texture_kind == KindAsset && arrayRead.texture == array.RendererIdentity,
            $"the world holds the Texture2DArray as the asset the host draws: kind={arrayRead.texture_kind} texture={arrayRead.texture}");
    }

    private static void LayeredTexturesAreBlitAndCopySources()
    {
        var (cube, array, target) = (new Cubemap(2, TextureFormat.RGBA32, false), new Texture2DArray(2, 2, 3, TextureFormat.RGBA32, false), Target());
        var buffer = new CommandBuffer();
        buffer.Blit(new RenderTargetIdentifier(cube), target);
        buffer.CopyTexture(new RenderTargetIdentifier(array), 2, 0, target, 0, 0);
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        var id = target.WorldId;
        var expected = new[]
        {
            $"Blit source={KindAsset}:{cube.RendererIdentity} target={id} material=0 pass=-1",
            $"Copy source={KindAsset}:{array.RendererIdentity}[2,0] target={id}[0,0]",
        };
        var taken = CommandWireReader.Take();
        Expect.That(taken.SequenceEqual(expected), $"records:\n  {string.Join("\n  ", taken)}\nexpected\n  {string.Join("\n  ", expected)}");
        var empty = new CommandBuffer();
        empty.Blit(new RenderTargetIdentifier(new Texture2D(0, 0)), target);
        Throws<NotSupportedException>(() => Graphics.ExecuteCommandBuffer(empty), "a blit of a texture the world cannot send", "CommandBuffer.Blit", "Texture2D");
        var emptyCopy = new CommandBuffer();
        emptyCopy.CopyTexture(new RenderTargetIdentifier(new Texture2D(0, 0)), 0, 0, target, 0, 0);
        Throws<NotSupportedException>(() => Graphics.ExecuteCommandBuffer(emptyCopy), "a copy of a texture the world cannot send", "CommandBuffer.CopyTexture", "Texture2D");
        CommandWireReader.Take();
    }

    private static void ALayeredTextureIsNeverARenderTarget()
    {
        var buffer = new CommandBuffer();
        foreach (Texture texture in new Texture[] { new Cubemap(2, TextureFormat.RGBA32, false), new Texture2DArray(2, 2, 3, TextureFormat.RGBA32, false) })
        {
            var name = texture.GetType().Name;
            Throws<NotSupportedException>(() => buffer.SetRenderTarget(new RenderTargetIdentifier(texture)), $"a {name} as a render target", name, "render target");
            Throws<NotSupportedException>(() => buffer.Blit(Target(), new RenderTargetIdentifier(texture)), $"a blit into a {name}", "render texture");
        }
        Expect.Equal(0, buffer.commandCount, "a refused record leaves the buffer as it was");
    }

    private static void ScalarGlobalsPersist()
    {
        var matrix = new Matrix4x4(new Vector4(1, 2, 3, 4), new Vector4(5, 6, 7, 8), new Vector4(9, 10, 11, 12), new Vector4(13, 14, 15, 16));
        var buffer = new CommandBuffer();
        buffer.SetGlobalFloat("_PgF", 1.5f);
        buffer.SetGlobalFloat(Shader.PropertyToID("_PgFId"), -2f);
        buffer.SetGlobalInt("_PgI", 3);
        buffer.SetGlobalInteger(Shader.PropertyToID("_PgInteger"), 4);
        buffer.SetGlobalVector("_PgV", new Vector4(1, 2, 3, 4));
        buffer.SetGlobalColor("_PgC", new Color(0.25f, 0.5f, 0.75f, 1f));
        buffer.SetGlobalColor(Shader.PropertyToID("_PgCId"), new Color(1, 0, 0, 1));
        buffer.SetGlobalMatrix("_PgM", matrix);
        RunAndLetTheTakeEnd(buffer);
        Expect.Near(1.5f, Shader.GetGlobalFloat("_PgF"), "float");
        Expect.Near(-2f, Shader.GetGlobalFloat("_PgFId"), "float by id");
        Expect.Equal(3, Shader.GetGlobalInt("_PgI"), "int");
        Expect.Equal(4, Shader.GetGlobalInteger("_PgInteger"), "integer by id");
        Expect.Equal(new Vector4(1, 2, 3, 4), Shader.GetGlobalVector("_PgV"), "vector");
        Expect.Equal((Vector4)new Color(0.25f, 0.5f, 0.75f, 1f), Shader.GetGlobalVector("_PgC"), "color");
        Expect.Equal((Vector4)new Color(1, 0, 0, 1), Shader.GetGlobalVector("_PgCId"), "color by id");
        Expect.Equal(matrix, Shader.GetGlobalMatrix("_PgM"), "matrix");
        var (f, f2, i, integer) = (Read("_PgF"), Read("_PgFId"), Read("_PgI"), Read("_PgInteger"));
        Expect.That((f.present & HasFloat) != 0 && f.float_value == 1.5f && f2.float_value == -2f, "the world holds the floats");
        Expect.That((i.present & HasFloat) != 0 && i.float_value == 3f, "SetGlobalInt is a float in the world, as Shader.SetGlobalInt makes it");
        Expect.That((integer.present & HasInt) != 0 && integer.int_value == 4, "SetGlobalInteger is an int in the world");
        var (v, c, m) = (Read("_PgV"), Read("_PgC"), Read("_PgM"));
        Expect.That((v.present & HasVector) != 0 && (v.vector_x, v.vector_y, v.vector_z, v.vector_w) == (1, 2, 3, 4), "the world holds the vector");
        Expect.That((c.present & HasVector) != 0 && (c.vector_x, c.vector_y, c.vector_z, c.vector_w) == (0.25f, 0.5f, 0.75f, 1f), "the world holds the color as a vector");
        Expect.That((m.present & HasMatrix) != 0, "the world holds the matrix");
    }

    private static void KeywordsPersist()
    {
        var keyword = new GlobalKeyword("PERSIST_GLOBAL");
        var buffer = new CommandBuffer();
        buffer.EnableShaderKeyword("PERSIST_BY_NAME");
        buffer.EnableKeyword(keyword);
        buffer.EnableShaderKeyword("PERSIST_OFF");
        buffer.DisableShaderKeyword("PERSIST_OFF");
        buffer.SetKeyword(new GlobalKeyword("PERSIST_SET"), true);
        RunAndLetTheTakeEnd(buffer);
        foreach (var name in new[] { "PERSIST_BY_NAME", "PERSIST_GLOBAL", "PERSIST_SET" })
            Expect.That(Shader.IsKeywordEnabled(name) && WorldGraphicsTests.KeywordOn(Global, GlobalLink, name), $"{name} is enabled in Shader and in the world");
        Expect.That(!Shader.IsKeywordEnabled("PERSIST_OFF") && !WorldGraphicsTests.KeywordOn(Global, GlobalLink, "PERSIST_OFF"), "a keyword disabled later in the buffer is off in both");
        var off = new CommandBuffer();
        off.DisableKeyword(keyword);
        off.DisableShaderKeyword("PERSIST_BY_NAME");
        off.SetKeyword(new GlobalKeyword("PERSIST_SET"), false);
        RunAndLetTheTakeEnd(off);
        foreach (var name in new[] { "PERSIST_BY_NAME", "PERSIST_GLOBAL", "PERSIST_SET" })
            Expect.That(!Shader.IsKeywordEnabled(name) && !WorldGraphicsTests.KeywordOn(Global, GlobalLink, name), $"{name} is disabled in Shader and in the world");
    }

    private static void ArraysPersist()
    {
        var vectors = new[] { new Vector4(1, 2, 3, 4), new Vector4(5, 6, 7, 8) };
        var matrices = new[] { new Matrix4x4(new Vector4(1, 2, 3, 4), new Vector4(5, 6, 7, 8), new Vector4(9, 10, 11, 12), new Vector4(13, 14, 15, 16)) };
        var buffer = new CommandBuffer();
        buffer.SetGlobalVectorArray("_PgVecs", vectors);
        buffer.SetGlobalMatrixArray(Shader.PropertyToID("_PgMats"), new List<Matrix4x4>(matrices));
        RunAndLetTheTakeEnd(buffer);
        CommandWireReader.AppendMarker();
        var snapshot = GlobalArrayTests.Snapshot();
        Expect.That(Shader.GetGlobalVectorArray("_PgVecs") is { } held && held.SequenceEqual(vectors), "the managed store holds the vector array");
        Expect.That(Shader.GetGlobalMatrixArray("_PgMats") is { } heldMatrices && heldMatrices.SequenceEqual(matrices), "the managed store holds the matrix array");
        Expect.That(GlobalArrayTests.Count(snapshot, GlobalArrayTests.VectorEntry("_PgVecs", vectors)) == 1, "the snapshot's global state holds the vector array");
        Expect.That(GlobalArrayTests.Count(snapshot, GlobalArrayTests.MatrixEntry("_PgMats", matrices)) == 1, "the snapshot's global state holds the matrix array");
    }

    private static void ATemporaryGlobalStaysRecordedWork()
    {
        var id = Shader.PropertyToID("_PgTempRt");
        var buffer = new CommandBuffer();
        buffer.SetGlobalTexture("_PgTemp", new RenderTargetIdentifier(id));
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        var taken = CommandWireReader.Take();
        Expect.Equal(1, taken.Length, "the temporary global is one ordered record");
        Expect.That(taken[0].StartsWith($"SetGlobal _PgTemp=texture temporary:{id} ", StringComparison.Ordinal), $"the record names the temporary, got {taken[0]}");
        Expect.That(Shader.GetGlobalTexture("_PgTemp") is null, "no managed texture holds a temporary");
        Expect.That((Read("_PgTemp").present & HasTexture) == 0, "the world's global state holds no temporary");
    }

    private static void ValuesStillApplyInOrderWithOtherRecords()
    {
        var (first, second) = (Shader.PropertyToID("_PgFirst"), Shader.PropertyToID("_PgSecond"));
        var buffer = new CommandBuffer();
        buffer.SetGlobalFloat("_PgOrder", 1f);
        buffer.ReleaseTemporaryRT(first);
        buffer.SetGlobalFloat("_PgOrder", 2f);
        buffer.EnableShaderKeyword("PERSIST_ORDER");
        buffer.ReleaseTemporaryRT(second);
        buffer.DisableShaderKeyword("PERSIST_ORDER");
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        var taken = CommandWireReader.TakeOrdered();
        var expected = new[]
        {
            "SetGlobal _PgOrder=float (1)", $"Release {first}", "SetGlobal _PgOrder=float (2)", "Keyword PERSIST_ORDER=True", $"Release {second}", "Keyword PERSIST_ORDER=False",
        };
        Expect.That(taken.SequenceEqual(expected), $"ordered work:\n  {string.Join("\n  ", taken)}\nexpected\n  {string.Join("\n  ", expected)}");
        Expect.Near(2f, Shader.GetGlobalFloat("_PgOrder"), "the last value persists");
        Expect.That(!Shader.IsKeywordEnabled("PERSIST_ORDER"), "the last keyword state persists");
    }

    private static void ANonExecutedBufferChangesNothing()
    {
        var target = Target();
        var buffer = new CommandBuffer();
        buffer.SetGlobalFloat("_PgLazy", 7f);
        buffer.SetGlobalTexture("_PgLazyT", new RenderTargetIdentifier(target));
        buffer.EnableShaderKeyword("PERSIST_LAZY");
        var context = new ScriptableRenderContext();
        context.ExecuteCommandBuffer(buffer);
        Expect.That(Shader.GetGlobalFloat("_PgLazy") == 0f && Shader.GetGlobalTexture("_PgLazyT") is null && !Shader.IsKeywordEnabled("PERSIST_LAZY"),
            "recording and queueing set nothing");
        Expect.That((Read("_PgLazy").present & HasFloat) == 0 && (Read("_PgLazyT").present & HasTexture) == 0 && !WorldGraphicsTests.KeywordOn(Global, GlobalLink, "PERSIST_LAZY"),
            "the world holds nothing yet");
        context.Submit();
        Expect.That(Shader.GetGlobalFloat("_PgLazy") == 7f && ReferenceEquals(Shader.GetGlobalTexture("_PgLazyT"), target) && Shader.IsKeywordEnabled("PERSIST_LAZY"),
            "Submit runs the queued buffer and sets the globals");
        Expect.That(Read("_PgLazyT").texture == target.WorldId && WorldGraphicsTests.KeywordOn(Global, GlobalLink, "PERSIST_LAZY"), "and the world holds them");
        CommandWireReader.Take();
    }

    private static void ADestroyedTextureThrowsWhenTheBufferRuns()
    {
        var target = Target();
        var buffer = new CommandBuffer();
        buffer.SetGlobalTexture("_PgGone", new RenderTargetIdentifier(target));
        UnityEngine.Object.DestroyImmediate(target);
        Throws<InvalidOperationException>(() => Graphics.ExecuteCommandBuffer(buffer), "a render texture destroyed after recording");
        Expect.That(Shader.GetGlobalTexture("_PgGone") is null && (Read("_PgGone").present & HasTexture) == 0, "nothing was set");
        CommandWireReader.Take();
    }
}
