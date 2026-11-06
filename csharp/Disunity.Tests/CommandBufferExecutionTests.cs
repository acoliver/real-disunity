using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>What a command buffer records and what reaches the world when it is executed, read back from the world's offscreen
/// list the way a host takes it.</summary>
internal static class CommandBufferExecutionTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            NothingReachesTheWorldUntilTheBufferIsExecuted();
            Console.WriteLine("PASS: a command buffer records in order, sends nothing to the world until executed, and Clear empties it");
            ExecutingTwiceDeliversTwice();
            Console.WriteLine("PASS: Graphics.ExecuteCommandBuffer delivers the commands in order and again each time it runs");
            ContextExecutesThenSubmits();
            Console.WriteLine("PASS: ScriptableRenderContext.ExecuteCommandBuffer queues commands in order and Submit sends them once");
            TargetsResolveByKind();
            Console.WriteLine("PASS: a RenderTargetIdentifier resolves to a render texture, a temporary, the camera target or none");
            UnsupportedTargetsNameThemselves();
            Console.WriteLine("PASS: a render target with no world form throws NotSupportedException naming it");
            SetRenderTargetOverloads();
            Console.WriteLine("PASS: SetRenderTarget records its colors, depth, mip level, cube face and depth slice for each overload");
            ClearOverloads();
            Console.WriteLine("PASS: ClearRenderTarget records which parts clear, the color, the depth and the stencil");
            GlobalsAreRecordedInOrder();
            Console.WriteLine("PASS: SetGlobal values, textures and keywords are recorded by name and id and not folded into the global state");
            TemporariesCarryTheirDescriptions();
            Console.WriteLine("PASS: GetTemporaryRT and ReleaseTemporaryRT record the description the arguments give");
            BlitAndCopyRecordRenderTextures();
            Console.WriteLine("PASS: CommandBuffer.Blit and CopyTexture record render texture draws and refuse targets they cannot hold");
            ReleasedAndNullBuffersAreRefused();
            Console.WriteLine("PASS: executing a null or released command buffer throws");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static string[] Taken() => CommandWireReader.Take();

    private static RenderTexture Target(int size = 16, int depth = 0)
    {
        var texture = new RenderTexture(size, size, depth, RenderTextureFormat.ARGB32);
        texture.Create();
        return texture;
    }

    private static void Same(string[] expected, string[] actual, string what) =>
        Expect.That(expected.SequenceEqual(actual), $"{what}: expected\n  {string.Join("\n  ", expected)}\ngot\n  {string.Join("\n  ", actual)}");

    private static void NothingReachesTheWorldUntilTheBufferIsExecuted()
    {
        Taken();
        var buffer = new CommandBuffer();
        buffer.EnableShaderKeyword("FIRST");
        buffer.SetGlobalFloat("_Second", 2f);
        buffer.DisableShaderKeyword("FIRST");
        Expect.Equal(3, buffer.commandCount, "three commands are held");
        Expect.Equal(0, Taken().Length, "nothing reaches the world while recording");
        Graphics.ExecuteCommandBuffer(buffer);
        Same(["Keyword FIRST=True", "SetGlobal _Second=float (2)", "Keyword FIRST=False"], Taken(), "executed in recorded order");
        buffer.Clear();
        Expect.Equal(0, buffer.commandCount, "Clear empties the buffer");
        Graphics.ExecuteCommandBuffer(buffer);
        Expect.Equal(0, Taken().Length, "an emptied buffer sends nothing");
        buffer.EnableShaderKeyword("AFTER");
        Graphics.ExecuteCommandBuffer(buffer);
        Same(["Keyword AFTER=True"], Taken(), "a buffer records again after Clear");
    }

    private static void ExecutingTwiceDeliversTwice()
    {
        var buffer = new CommandBuffer();
        buffer.SetGlobalInt("_A", 1);
        buffer.SetGlobalInt("_B", 2);
        Graphics.ExecuteCommandBuffer(buffer);
        Graphics.ExecuteCommandBuffer(buffer);
        buffer.SetGlobalInt("_C", 3);
        Same(["SetGlobal _A=int 1", "SetGlobal _B=int 2", "SetGlobal _A=int 1", "SetGlobal _B=int 2"], Taken(), "two executions");
        Graphics.ExecuteCommandBuffer(buffer);
        Same(["SetGlobal _A=int 1", "SetGlobal _B=int 2", "SetGlobal _C=int 3"], Taken(), "a command recorded after an execution joins the next one");
    }

    private static void ContextExecutesThenSubmits()
    {
        var (first, second) = (new CommandBuffer(), new CommandBuffer());
        first.EnableShaderKeyword("A");
        first.EnableShaderKeyword("B");
        second.EnableShaderKeyword("C");
        var context = new ScriptableRenderContext();
        context.ExecuteCommandBuffer(first);
        context.ExecuteCommandBuffer(second);
        context.ExecuteCommandBuffer(first);
        Expect.Equal(5, ScriptableRenderContext.PendingCount, "five commands wait");
        Expect.Equal(0, Taken().Length, "nothing is sent before Submit");
        first.Clear();
        new ScriptableRenderContext().Submit();
        Same(["Keyword A=True", "Keyword B=True", "Keyword C=True", "Keyword A=True", "Keyword B=True"], Taken(), "submitted in order, copies taken at Execute");
        Expect.Equal(0, ScriptableRenderContext.PendingCount, "Submit empties the list");
        context.Submit();
        Expect.Equal(0, Taken().Length, "a second Submit sends nothing");
        Expect.That(second.commandCount == 1, "the buffer keeps its commands after being queued");
        Throws<ArgumentNullException>(() => context.ExecuteCommandBuffer(null!), "a null buffer");
    }

    private static void TargetsResolveByKind()
    {
        var texture = Target();
        var named = Shader.PropertyToID("_Named");
        var buffer = new CommandBuffer();
        buffer.SetRenderTarget(texture);
        buffer.SetRenderTarget(new RenderTargetIdentifier(named));
        buffer.SetRenderTarget(new RenderTargetIdentifier("_Named"));
        buffer.SetRenderTarget(BuiltinRenderTextureType.CameraTarget);
        buffer.SetRenderTarget(BuiltinRenderTextureType.None);
        Graphics.ExecuteCommandBuffer(buffer);
        var id = texture.WorldId;
        Same([
            $"SetRenderTarget colors=[Render:{id}] depth=- mip=0 face=-1 slice=0",
            $"SetRenderTarget colors=[Temp:{named}] depth=- mip=0 face=-1 slice=0",
            $"SetRenderTarget colors=[Temp:{named}] depth=- mip=0 face=-1 slice=0",
            "SetRenderTarget colors=[Camera] depth=- mip=0 face=-1 slice=0",
            "SetRenderTarget colors=[None] depth=- mip=0 face=-1 slice=-1",
        ], Taken(), "each kind of target");
        var uncreated = new RenderTexture(8, 8, 0);
        var late = new CommandBuffer();
        late.SetRenderTarget(uncreated);
        Expect.That(!uncreated.IsCreated(), "recording does not create the texture");
        Graphics.ExecuteCommandBuffer(late);
        Expect.That(uncreated.IsCreated(), "executing creates it, as a blit does");
        Same([$"SetRenderTarget colors=[Render:{uncreated.WorldId}] depth=- mip=0 face=-1 slice=0"], Taken(), "a texture made at execution");
        UnityEngine.Object.DestroyImmediate(texture);
        Throws<InvalidOperationException>(() => Graphics.ExecuteCommandBuffer(buffer), "a texture destroyed after recording");
        Taken();
        Throws<ArgumentException>(() => new CommandBuffer().SetRenderTarget(new RenderTargetIdentifier(texture)), "a destroyed texture identifier");
    }

    private static void UnsupportedTargetsNameThemselves()
    {
        var buffer = new CommandBuffer();
        foreach (var type in new[] { BuiltinRenderTextureType.Depth, BuiltinRenderTextureType.MotionVectors, BuiltinRenderTextureType.GBuffer0, BuiltinRenderTextureType.CurrentActive })
            Throws<NotSupportedException>(() => buffer.SetRenderTarget(type), $"target {type}", $"BuiltinRenderTextureType.{type}", "CommandBuffer.SetRenderTarget");
        Throws<NotSupportedException>(() => buffer.SetRenderTarget(new Texture2D(2, 2)), "a plain texture", "BindableTexture");
        Throws<NotSupportedException>(() => buffer.SetRenderTarget(Target().colorBuffer), "a render buffer", "BufferPtr");
        Throws<NotSupportedException>(() => buffer.SetRenderTarget(BuiltinRenderTextureType.CameraTarget, BuiltinRenderTextureType.Depth), "a depth target", "Depth");
        Throws<NotSupportedException>(() => buffer.SetGlobalTexture("_T", BuiltinRenderTextureType.GBuffer1), "a global texture", "GBuffer1");
        Expect.Equal(0, buffer.commandCount, "a refused record leaves the buffer as it was");
    }

    private static void SetRenderTargetOverloads()
    {
        var (color, depth) = (Target(), Target(16, 24));
        var cube = new RenderTargetIdentifier(color, 3, CubemapFace.NegativeY, 7);
        var named = Shader.PropertyToID("_Depth");
        var buffer = new CommandBuffer();
        buffer.SetRenderTarget(cube);
        buffer.SetRenderTarget(color, 2);
        buffer.SetRenderTarget(color, 2, CubemapFace.PositiveZ);
        buffer.SetRenderTarget(color, 2, CubemapFace.PositiveZ, 5);
        buffer.SetRenderTarget(color, RenderBufferLoadAction.DontCare, RenderBufferStoreAction.DontCare);
        buffer.SetRenderTarget(color, depth);
        buffer.SetRenderTarget(color, depth, 1);
        buffer.SetRenderTarget(color, depth, 1, CubemapFace.NegativeX);
        buffer.SetRenderTarget(color, depth, 1, CubemapFace.NegativeX, -1);
        buffer.SetRenderTarget(color, RenderBufferLoadAction.Load, RenderBufferStoreAction.Store, new RenderTargetIdentifier(named), RenderBufferLoadAction.Clear, RenderBufferStoreAction.DontCare);
        buffer.SetRenderTarget(new RenderTargetIdentifier[] { color, new RenderTargetIdentifier(named) }, depth);
        buffer.SetRenderTarget(new RenderTargetIdentifier[] { color }, depth, 4, CubemapFace.Unknown, 2);
        Graphics.ExecuteCommandBuffer(buffer);
        var (c, d) = (color.WorldId, depth.WorldId);
        Same([
            $"SetRenderTarget colors=[Render:{c}] depth=- mip=3 face=3 slice=7",
            $"SetRenderTarget colors=[Render:{c}] depth=- mip=2 face=-1 slice=0",
            $"SetRenderTarget colors=[Render:{c}] depth=- mip=2 face=4 slice=0",
            $"SetRenderTarget colors=[Render:{c}] depth=- mip=2 face=4 slice=5",
            $"SetRenderTarget colors=[Render:{c}] depth=- mip=0 face=-1 slice=0",
            $"SetRenderTarget colors=[Render:{c}] depth=Render:{d} mip=0 face=-1 slice=0",
            $"SetRenderTarget colors=[Render:{c}] depth=Render:{d} mip=1 face=-1 slice=0",
            $"SetRenderTarget colors=[Render:{c}] depth=Render:{d} mip=1 face=1 slice=0",
            $"SetRenderTarget colors=[Render:{c}] depth=Render:{d} mip=1 face=1 slice=-1",
            $"SetRenderTarget colors=[Render:{c}] depth=Temp:{named} mip=0 face=-1 slice=0",
            $"SetRenderTarget colors=[Render:{c},Temp:{named}] depth=Render:{d} mip=0 face=-1 slice=0",
            $"SetRenderTarget colors=[Render:{c}] depth=Render:{d} mip=4 face=-1 slice=2",
        ], Taken(), "each overload");
        Throws<ArgumentOutOfRangeException>(() => buffer.SetRenderTarget(color, -1), "a negative mip level");
        Throws<ArgumentNullException>(() => buffer.SetRenderTarget((RenderTargetIdentifier[])null!, depth), "null colors");
    }

    private static void ClearOverloads()
    {
        var buffer = new CommandBuffer();
        var color = new Color(0.25f, 0.5f, 0.75f, 1f);
        buffer.ClearRenderTarget(true, false, color);
        buffer.ClearRenderTarget(false, true, color, 0.5f);
        buffer.ClearRenderTarget(RTClearFlags.All, color, 0.25f, 9);
        buffer.ClearRenderTarget(RTClearFlags.Stencil, color, 1f, 3);
        buffer.ClearRenderTarget(RTClearFlags.Color3, color, 1f, 0);
        buffer.ClearRenderTarget(RTClearFlags.DepthStencil, color, 0f, 255);
        Graphics.ExecuteCommandBuffer(buffer);
        Same([
            "Clear flags=D color=(0.25 0.5 0.75 1) depth=1 stencil=0",
            "Clear flags=C color=(0.25 0.5 0.75 1) depth=0.5 stencil=0",
            "Clear flags=CDS color=(0.25 0.5 0.75 1) depth=0.25 stencil=9",
            "Clear flags=S color=(0.25 0.5 0.75 1) depth=1 stencil=3",
            "Clear flags=C color=(0.25 0.5 0.75 1) depth=1 stencil=0",
            "Clear flags=DS color=(0.25 0.5 0.75 1) depth=0 stencil=255",
        ], Taken(), "clears");
        var bad = new CommandBuffer();
        bad.ClearRenderTarget(true, true, new Color(float.NaN, 0, 0, 0));
        Throws<DisunityException>(() => Graphics.ExecuteCommandBuffer(bad), "a clear value that is not finite");
        Taken();
    }

    private static void GlobalsAreRecordedInOrder()
    {
        var texture = Target();
        var id = Shader.PropertyToID("_ByIdFloat");
        var buffer = new CommandBuffer();
        buffer.SetGlobalFloat("_F", 1.5f);
        buffer.SetGlobalFloat(id, -2f);
        buffer.SetGlobalInt("_I", 3);
        buffer.SetGlobalInteger(Shader.PropertyToID("_I2"), 4);
        buffer.SetGlobalVector("_V", new Vector4(1, 2, 3, 4));
        buffer.SetGlobalColor("_C", new Color(0.1f, 0.2f, 0.3f, 0.4f));
        buffer.SetGlobalColor(Shader.PropertyToID("_C2"), new Color(1, 0, 0, 1));
        buffer.SetGlobalMatrix("_M", new Matrix4x4(new Vector4(1, 2, 3, 4), new Vector4(5, 6, 7, 8), new Vector4(9, 10, 11, 12), new Vector4(13, 14, 15, 16)));
        buffer.SetGlobalTexture("_T", texture);
        buffer.SetGlobalTexture(Shader.PropertyToID("_T2"), BuiltinRenderTextureType.None);
        var keyword = new GlobalKeyword("GLOBAL_FOG");
        buffer.EnableKeyword(keyword);
        buffer.DisableKeyword(keyword);
        buffer.SetKeyword(keyword, true);
        buffer.EnableShaderKeyword("BY_NAME");
        buffer.DisableShaderKeyword("BY_NAME");
        var before = Shader.GetGlobalFloat("_F");
        Graphics.ExecuteCommandBuffer(buffer);
        Expect.Near(before, Shader.GetGlobalFloat("_F"), "executing leaves the managed global alone");
        Same([
            "SetGlobal _F=float (1.5)",
            "SetGlobal _ByIdFloat=float (-2)",
            "SetGlobal _I=int 3",
            "SetGlobal _I2=int 4",
            "SetGlobal _V=vector (1 2 3 4)",
            "SetGlobal _C=vector (0.1 0.2 0.3 0.4)",
            "SetGlobal _C2=vector (1 0 0 1)",
            "SetGlobal _M=matrix (1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16)",
            $"SetGlobal _T=texture render:{texture.WorldId} scale=(1 1) offset=(0 0)",
            "SetGlobal _T2=texture none scale=(1 1) offset=(0 0)",
            "Keyword GLOBAL_FOG=True",
            "Keyword GLOBAL_FOG=False",
            "Keyword GLOBAL_FOG=True",
            "Keyword BY_NAME=True",
            "Keyword BY_NAME=False",
        ], Taken(), "globals");
        Throws<ArgumentException>(() => buffer.SetGlobalFloat(unchecked((int)0x7ABCDEF1), 1f), "an id with no name");
        Throws<ArgumentNullException>(() => buffer.EnableShaderKeyword(null!), "a null keyword");
        Throws<ArgumentNullException>(() => buffer.SetGlobalFloat((string)null!, 1f), "a null name");
        var empty = new CommandBuffer();
        empty.SetGlobalFloat("", 1f);
        Throws<DisunityException>(() => Graphics.ExecuteCommandBuffer(empty), "an empty name reaches the world and is refused there");
        Taken();
    }

    private static void TemporariesCarryTheirDescriptions()
    {
        var buffer = new CommandBuffer();
        var expected = new RenderTextureDescriptor(64, 32, RenderTextureFormat.ARGBHalf, 24, UnityEngine.Texture.GenerateAllMips, RenderTextureReadWrite.Linear);
        buffer.GetTemporaryRT(11, 64, 32, 24, FilterMode.Trilinear, RenderTextureFormat.ARGBHalf, RenderTextureReadWrite.Linear);
        buffer.GetTemporaryRT(12, 8, 8);
        buffer.GetTemporaryRT(13, 4, 4, 16, FilterMode.Bilinear, GraphicsFormat.R8G8B8A8_UNorm);
        buffer.GetTemporaryRT(14, new RenderTextureDescriptor(20, 10, GraphicsFormat.R16G16B16A16_SFloat, 0, 3), FilterMode.Bilinear);
        buffer.GetTemporaryRT(15, new RenderTextureDescriptor(6, 6, RenderTextureFormat.ARGB32, 0));
        buffer.GetTemporaryRT(16, -1, 0, 0);
        buffer.ReleaseTemporaryRT(11);
        buffer.ReleaseTemporaryRT(-5);
        Graphics.ExecuteCommandBuffer(buffer);
        var taken = Taken();
        var format = (int)expected.graphicsFormat;
        var defaults = (int)new RenderTextureDescriptor(8, 8, RenderTextureFormat.Default, 0).graphicsFormat;
        var defaultSrgb = new RenderTextureDescriptor(8, 8, RenderTextureFormat.Default, 0).sRGB;
        var rgba = (int)GraphicsFormat.R8G8B8A8_UNorm;
        var half = (int)GraphicsFormat.R16G16B16A16_SFloat;
        var argb = (int)new RenderTextureDescriptor(6, 6, RenderTextureFormat.ARGB32, 0).graphicsFormat;
        var screen = (Screen.width, Screen.height);
        Expect.Equal(8, taken.Length, "eight records");
        Expect.That(taken[0] == $"Allocate 11 64x32 format={format} depth=24 mips=1 srgb=False wrap=111 filter=2 aniso=1 bias=0", $"full allocation, got {taken[0]}");
        Expect.That(taken[1] == $"Allocate 12 8x8 format={defaults} depth=0 mips=1 srgb={defaultSrgb} wrap=111 filter=0 aniso=1 bias=0", $"defaults, got {taken[1]}");
        Expect.That(taken[2].StartsWith($"Allocate 13 4x4 format={rgba} depth=16 mips=1 srgb=False wrap=111 filter=1"), $"graphics format, got {taken[2]}");
        Expect.That(taken[3].StartsWith($"Allocate 14 20x10 format={half} depth=0 mips=3 srgb=False wrap=111 filter=1"), $"descriptor with filter, got {taken[3]}");
        Expect.That(taken[4].StartsWith($"Allocate 15 6x6 format={argb} depth=0") && taken[4].Contains("filter=0"), $"descriptor alone is point filtered, got {taken[4]}");
        Expect.That(taken[5].StartsWith($"Allocate 16 {screen.width}x{screen.height} format={defaults} depth=0"), $"a size of zero or below is the screen, got {taken[5]}");
        Expect.That(taken[6] == "Release 11" && taken[7] == "Release -5", "releases keep their ids");
        Throws<ArgumentException>(() => { var bad = new CommandBuffer(); bad.GetTemporaryRT(1, 4, 4, 0, FilterMode.Point, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Default, 3); Graphics.ExecuteCommandBuffer(bad); }, "anti-aliasing the world cannot hold");
        Taken();
    }

    private static void BlitAndCopyRecordRenderTextures()
    {
        var (a, b) = (Target(), Target());
        var mipped = new RenderTexture(16, 16, 0, RenderTextureFormat.ARGB32, 3);
        mipped.Create();
        var plain = new Texture2D(4, 4);
        var material = new Material(Material.SpriteLit);
        var buffer = new CommandBuffer();
        buffer.Blit(a, b);
        buffer.Blit(a, b, material);
        buffer.Blit(plain, b, material, 1);
        buffer.Blit(new RenderTargetIdentifier(a), new RenderTargetIdentifier(b), material, 0);
        buffer.CopyTexture(a, 0, 0, b, 0, 0);
        buffer.CopyTexture(mipped, 1, 2, b, 3, 0);
        Graphics.ExecuteCommandBuffer(buffer);
        var (ida, idb, mat) = (a.WorldId, b.WorldId, material.WorldLink.Id);
        var taken = Taken();
        Expect.Equal(6, taken.Length, "six records");
        Expect.That(taken[0] == $"Blit source=2:{ida} target={idb} material=0 pass=-1", $"plain blit, got {taken[0]}");
        Expect.That(taken[1] == $"Blit source=2:{ida} target={idb} material={mat} pass=-1", $"blit with material, got {taken[1]}");
        Expect.That(taken[2] == $"Blit source=1:{plain.RendererIdentity} target={idb} material={mat} pass=1", $"blit of a player texture, got {taken[2]}");
        Expect.That(taken[3] == $"Blit source=2:{ida} target={idb} material={mat} pass=0", $"blit by identifier, got {taken[3]}");
        Expect.That(taken[4] == $"Copy source=2:{ida}[0,0] target={idb}[0,0]", $"copy, got {taken[4]}");
        Expect.That(taken[5] == $"Copy source=2:{mipped.WorldId}[1,2] target={idb}[3,0]", $"copy of elements and mip levels, got {taken[5]}");
        Throws<NotSupportedException>(() => buffer.Blit(a, BuiltinRenderTextureType.CameraTarget), "a blit to the camera target", "CommandBuffer.Blit", "render texture");
        Throws<NotSupportedException>(() => buffer.Blit(new RenderTargetIdentifier("_Tmp"), b), "a blit from a temporary", "temporary render texture");
        Throws<NotSupportedException>(() => buffer.CopyTexture(a, 0, 0, "_Tmp", 0, 0), "a copy to a temporary", "CommandBuffer.CopyTexture");
        Throws<ArgumentException>(() => buffer.Blit(a, b, material, -2), "a pass below -1");
        Throws<ArgumentException>(() => buffer.CopyTexture(a, -1, 0, b, 0, 0), "a negative element");
    }

    private static void ReleasedAndNullBuffersAreRefused()
    {
        Throws<ArgumentNullException>(() => Graphics.ExecuteCommandBuffer(null!), "a null buffer");
        var buffer = new CommandBuffer();
        buffer.EnableShaderKeyword("K");
        buffer.Release();
        Throws<ObjectDisposedException>(() => Graphics.ExecuteCommandBuffer(buffer), "a released buffer");
        Throws<ObjectDisposedException>(() => new ScriptableRenderContext().ExecuteCommandBuffer(buffer), "a released buffer in a context");
        Throws<ObjectDisposedException>(() => buffer.EnableShaderKeyword("L"), "recording into a released buffer");
        Throws<ObjectDisposedException>(() => buffer.SetRenderTarget(BuiltinRenderTextureType.None), "binding in a released buffer");
        Throws<ObjectDisposedException>(() => buffer.GetTemporaryRT(1, 2, 2), "allocating in a released buffer");
        Expect.Equal(0, Taken().Length, "a refused execution sends nothing");
    }
}
