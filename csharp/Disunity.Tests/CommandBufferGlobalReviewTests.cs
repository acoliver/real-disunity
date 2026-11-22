using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Review tests for the globals a command buffer sets (#171): the cases the proposal's own tests do not pin.</summary>
internal static unsafe class CommandBufferGlobalReviewTests
{
    private const uint Global = 1, GlobalLink = 1;

    /// <summary>Runs every case, so one failure does not hide the others, and throws when any failed.</summary>
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var failures = new List<string>();
        try
        {
            Case("ATrailingGlobalStaysOrderedAfterTheLastRecord", ATrailingGlobalStaysOrderedAfterTheLastRecord, failures);
            Case("ATrailingKeywordStaysOrderedAfterTheLastRecord", ATrailingKeywordStaysOrderedAfterTheLastRecord, failures);
            Case("ABufferRunTwiceSetsItsGlobalsAgain", ABufferRunTwiceSetsItsGlobalsAgain, failures);
            Case("AClearedBufferKeepsWhatItSetAndSetsNewValues", AClearedBufferKeepsWhatItSetAndSetsNewValues, failures);
            Case("ADirectWriteBeforeAndAfterABufferKeepsItsPlace", ADirectWriteBeforeAndAfterABufferKeepsItsPlace, failures);
            Case("ADestroyedRenderTextureLeavesTheNextTakeWorking", ADestroyedRenderTextureLeavesTheNextTakeWorking, failures);
            Case("AReleasedRenderTextureStaysTheGlobal", AReleasedRenderTextureStaysTheGlobal, failures);
            Case("ARefusedNameStopsTheBufferAfterTheCommandsBeforeIt", ARefusedNameStopsTheBufferAfterTheCommandsBeforeIt, failures);
            Case("ADiscardedContextSetsNothing", ADiscardedContextSetsNothing, failures);
            Case("ASubmittedContextSetsEveryKind", ASubmittedContextSetsEveryKind, failures);
            Case("ATextureAndATemporaryUnderOneNameKeepTheirOrder", ATextureAndATemporaryUnderOneNameKeepTheirOrder, failures);
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
        if (failures.Count > 0) throw new InvalidOperationException($"{failures.Count} review cases failed: {string.Join(", ", failures)}");
    }

    private static void Case(string name, Action run, List<string> failures)
    {
        try
        {
            run();
            Console.WriteLine($"CASE-PASS: {name}");
        }
        catch (Exception error)
        {
            failures.Add(name);
            Console.WriteLine($"CASE-FAIL: {name}: {error.Message.ReplaceLineEndings(" | ")}");
        }
    }

    private static DisunityPropertyRead Read(string name) => WorldGraphicsTests.Read(Global, GlobalLink, name);

    private static RenderTexture Target()
    {
        var texture = new RenderTexture(16, 16, 0, RenderTextureFormat.ARGB32);
        texture.Create();
        return texture;
    }

    private static void ATrailingGlobalStaysOrderedAfterTheLastRecord()
    {
        var id = Shader.PropertyToID("_RvTrailRelease");
        var buffer = new CommandBuffer();
        buffer.ReleaseTemporaryRT(id);
        buffer.SetGlobalFloat("_RvTrail", 5f);
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        var taken = CommandWireReader.Take();
        var expected = new[] { $"Release {id}", "SetGlobal _RvTrail=float (5)" };
        Expect.That(taken.SequenceEqual(expected), $"work:\n  {string.Join("\n  ", taken)}\nexpected\n  {string.Join("\n  ", expected)}");
    }

    private static void ATrailingKeywordStaysOrderedAfterTheLastRecord()
    {
        var id = Shader.PropertyToID("_RvKeywordRelease");
        var buffer = new CommandBuffer();
        buffer.ReleaseTemporaryRT(id);
        buffer.EnableShaderKeyword("RV_TRAILING");
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        var taken = CommandWireReader.Take();
        var expected = new[] { $"Release {id}", "Keyword RV_TRAILING=True" };
        Expect.That(taken.SequenceEqual(expected), $"work:\n  {string.Join("\n  ", taken)}\nexpected\n  {string.Join("\n  ", expected)}");
    }

    private static void ABufferRunTwiceSetsItsGlobalsAgain()
    {
        var buffer = new CommandBuffer();
        buffer.SetGlobalFloat("_RvTwice", 1f);
        buffer.EnableShaderKeyword("RV_TWICE");
        Graphics.ExecuteCommandBuffer(buffer);
        Shader.SetGlobalFloat("_RvTwice", 9f);
        Shader.DisableKeyword("RV_TWICE");
        Expect.Near(9f, Shader.GetGlobalFloat("_RvTwice"), "the direct write between the runs");
        Graphics.ExecuteCommandBuffer(buffer);
        Expect.Near(1f, Shader.GetGlobalFloat("_RvTwice"), "the second run sets the float again");
        Expect.That(Shader.IsKeywordEnabled("RV_TWICE") && WorldGraphicsTests.KeywordOn(Global, GlobalLink, "RV_TWICE"), "and the keyword again, in the world too");
        Expect.Equal(2, buffer.commandCount, "running keeps the commands");
        CommandWireReader.Take();
    }

    private static void AClearedBufferKeepsWhatItSetAndSetsNewValues()
    {
        var buffer = new CommandBuffer();
        buffer.SetGlobalVector("_RvCleared", new Vector4(1, 2, 3, 4));
        Graphics.ExecuteCommandBuffer(buffer);
        buffer.Clear();
        Graphics.ExecuteCommandBuffer(buffer);
        Expect.Equal(new Vector4(1, 2, 3, 4), Shader.GetGlobalVector("_RvCleared"), "Clear and a run of the empty buffer leave the global");
        var read = Read("_RvCleared");
        Expect.That(read.vector_x == 1 && read.vector_w == 4, "the world still holds it");
        buffer.SetGlobalVector("_RvCleared", new Vector4(5, 6, 7, 8));
        Graphics.ExecuteCommandBuffer(buffer);
        Expect.Equal(new Vector4(5, 6, 7, 8), Shader.GetGlobalVector("_RvCleared"), "the re-recorded buffer sets the new value");
        Expect.That(Read("_RvCleared").vector_x == 5, "and so does the world");
        CommandWireReader.Take();
    }

    private static void ADirectWriteBeforeAndAfterABufferKeepsItsPlace()
    {
        var (first, second) = (Shader.PropertyToID("_RvMixFirst"), Shader.PropertyToID("_RvMixSecond"));
        CommandWireReader.Take();
        Shader.SetGlobalFloat("_RvMix", 1f);
        var buffer = new CommandBuffer();
        buffer.ReleaseTemporaryRT(first);
        buffer.SetGlobalFloat("_RvMix", 2f);
        buffer.ReleaseTemporaryRT(second);
        Graphics.ExecuteCommandBuffer(buffer);
        Shader.SetGlobalFloat("_RvMix", 3f);
        var taken = CommandWireReader.TakeOrdered();
        var expected = new[]
        {
            "SetGlobal _RvMix=float (1)", $"Release {first}", "SetGlobal _RvMix=float (2)", $"Release {second}", "SetGlobal _RvMix=float (3)",
        };
        Expect.That(taken.SequenceEqual(expected), $"work:\n  {string.Join("\n  ", taken)}\nexpected\n  {string.Join("\n  ", expected)}");
        Expect.Near(3f, Shader.GetGlobalFloat("_RvMix"), "the last write wins");
    }

    private static void ADestroyedRenderTextureLeavesTheNextTakeWorking()
    {
        var target = Target();
        var id = target.WorldId;
        var buffer = new CommandBuffer();
        buffer.SetGlobalTexture("_RvDead", new RenderTargetIdentifier(target));
        Graphics.ExecuteCommandBuffer(buffer);
        CommandWireReader.Take();
        Expect.That(Read("_RvDead").texture == id, "the world holds the render texture while it lives");
        UnityEngine.Object.DestroyImmediate(target);
        var read = Read("_RvDead");
        // The state keeps the id; the world reads a destroyed render texture as none when it orders the value (World::live_texture).
        Expect.That(read.texture == id, $"the state keeps the id of the destroyed texture: kind={read.texture_kind} texture={read.texture} was {id}");
        CommandWireReader.AppendMarker();
        CommandWireReader.Take();
        // The world keeps the destroyed texture's id in this global; later suites expect a world with no unsent texture.
        Shader.SetGlobalTexture("_RvDead", (Texture?)null);
        CommandWireReader.Take();
    }

    private static void AReleasedRenderTextureStaysTheGlobal()
    {
        var target = Target();
        var buffer = new CommandBuffer();
        buffer.SetGlobalTexture("_RvReleased", new RenderTargetIdentifier(target));
        Graphics.ExecuteCommandBuffer(buffer);
        target.Release();
        Expect.That(ReferenceEquals(Shader.GetGlobalTexture("_RvReleased"), target), "the managed global is the released texture");
        Expect.That(Read("_RvReleased").texture == target.WorldId, "the world still names it, as it keeps its id across Release and Create");
        target.Create();
        CommandWireReader.Take();
    }

    private static void ARefusedNameStopsTheBufferAfterTheCommandsBeforeIt()
    {
        var buffer = new CommandBuffer();
        buffer.SetGlobalFloat("_RvOk", 1f);
        buffer.SetGlobalFloat("", 2f);
        buffer.SetGlobalFloat("_RvAfter", 3f);
        Throws<ArgumentException>(() => Graphics.ExecuteCommandBuffer(buffer), "an empty name", "1 to 256");
        Expect.Near(1f, Shader.GetGlobalFloat("_RvOk"), "the command before the refused one ran");
        Expect.Near(0f, Shader.GetGlobalFloat("_RvAfter"), "the command after it did not");
        CommandWireReader.Take();
    }

    private static void ADiscardedContextSetsNothing()
    {
        var buffer = new CommandBuffer();
        buffer.SetGlobalFloat("_RvDiscard", 4f);
        buffer.EnableShaderKeyword("RV_DISCARD");
        var context = new ScriptableRenderContext();
        context.ExecuteCommandBuffer(buffer);
        ScriptableRenderContext.DiscardPending();
        context.Submit();
        Expect.That(Shader.GetGlobalFloat("_RvDiscard") == 0f && !Shader.IsKeywordEnabled("RV_DISCARD"), "work a pipeline never submitted sets nothing");
        CommandWireReader.Take();
    }

    private static void ASubmittedContextSetsEveryKind()
    {
        var target = Target();
        var matrix = new Matrix4x4(new Vector4(1, 2, 3, 4), new Vector4(5, 6, 7, 8), new Vector4(9, 10, 11, 12), new Vector4(13, 14, 15, 16));
        var vectors = new[] { new Vector4(1, 2, 3, 4) };
        var buffer = new CommandBuffer();
        buffer.SetGlobalFloat("_RvCxF", 1f);
        buffer.SetGlobalInt("_RvCxI", 2);
        buffer.SetGlobalInteger("_RvCxInteger", 3);
        buffer.SetGlobalVector("_RvCxV", new Vector4(1, 2, 3, 4));
        buffer.SetGlobalColor("_RvCxC", new Color(0.5f, 0.5f, 0.5f, 1f));
        buffer.SetGlobalMatrix("_RvCxM", matrix);
        buffer.SetGlobalVectorArray("_RvCxVA", vectors);
        buffer.SetGlobalMatrixArray("_RvCxMA", new[] { matrix });
        buffer.EnableShaderKeyword("RV_CTX");
        buffer.SetGlobalTexture("_RvCxT", new RenderTargetIdentifier(target));
        var context = new ScriptableRenderContext();
        context.ExecuteCommandBuffer(buffer);
        context.Submit();
        CommandWireReader.Take();
        CommandWireReader.AppendMarker();
        var snapshot = GlobalArrayTests.Snapshot();
        Expect.That(Shader.GetGlobalFloat("_RvCxF") == 1f && Shader.GetGlobalFloat("_RvCxI") == 2f && Shader.GetGlobalInteger("_RvCxInteger") == 3, "scalars");
        Expect.That(Shader.GetGlobalVector("_RvCxV") == new Vector4(1, 2, 3, 4) && Shader.GetGlobalMatrix("_RvCxM") == matrix, "vector and matrix");
        Expect.That(Shader.GetGlobalVector("_RvCxC") == (Vector4)new Color(0.5f, 0.5f, 0.5f, 1f), "color");
        Expect.That(ReferenceEquals(Shader.GetGlobalTexture("_RvCxT"), target) && Read("_RvCxT").texture == target.WorldId, "texture");
        Expect.That(Shader.IsKeywordEnabled("RV_CTX") && WorldGraphicsTests.KeywordOn(Global, GlobalLink, "RV_CTX"), "keyword");
        Expect.That(GlobalArrayTests.Count(snapshot, GlobalArrayTests.VectorEntry("_RvCxVA", vectors)) == 1, "vector array in the world");
        CommandWireReader.Take();
    }

    private static void ATextureAndATemporaryUnderOneNameKeepTheirOrder()
    {
        var (target, temporary) = (Target(), Shader.PropertyToID("_RvTempRt"));
        var buffer = new CommandBuffer();
        buffer.SetGlobalTexture("_RvBoth", new RenderTargetIdentifier(target));
        buffer.SetGlobalTexture("_RvBoth", new RenderTargetIdentifier(temporary));
        CommandWireReader.Take();
        Graphics.ExecuteCommandBuffer(buffer);
        var taken = CommandWireReader.Take();
        Expect.That(taken.Length == 2 && taken[0].Contains($"texture render:{target.WorldId}", StringComparison.Ordinal)
            && taken[1].Contains($"texture temporary:{temporary}", StringComparison.Ordinal), $"work: {string.Join(" | ", taken)}");
        Expect.That(ReferenceEquals(Shader.GetGlobalTexture("_RvBoth"), target), "the managed global is the render texture the temporary does not replace");
    }
}
