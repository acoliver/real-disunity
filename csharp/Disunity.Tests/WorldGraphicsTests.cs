using System.Text;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>What the managed Material, Shader, RenderTexture, Camera and Graphics hand to the native world: the world's copy
/// follows each setter, and offscreen work is recorded in call order.</summary>
internal static unsafe class WorldGraphicsTests
{
    private const uint Material = 0, Global = 1, Block = 2;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            MaterialSettersReachTheWorld();
            Console.WriteLine("PASS: a material's float, integer, color, vector, texture, offset, scale, keyword, queue and pass setters reach its world material");
            MaterialStateBeforeItsFirstUseIsSentWhole();
            Console.WriteLine("PASS: a material that was set before it had a world material is sent whole when it gets one");
            MaterialCopiesAndShaderChangesFollow();
            Console.WriteLine("PASS: a material copy, a shader change and a cleared property store reach the world material");
            GlobalsReachTheWorld();
            Console.WriteLine("PASS: Shader.SetGlobal* and global keywords reach the world, including values set before the world was loaded");
            RenderTexturesReachTheWorld();
            Console.WriteLine("PASS: a RenderTexture's description reaches the world on Create, keeps its id across Release and goes with Destroy");
            RendererSlotsAndBlocksReachTheWorld();
            Console.WriteLine("PASS: a mesh renderer sends the world material of each slot and its property block");
            CameraRenderRecordsOffscreenWork();
            Console.WriteLine("PASS: Camera.Render with a target texture records CameraRender, and without one records nothing");
            BlitAndCopyRecordInOrder();
            Console.WriteLine("PASS: Graphics.Blit and Graphics.CopyTexture record offscreen work in call order");
            UnsupportedOverloadsThrowNamingTheMember();
            Console.WriteLine("PASS: Graphics.Blit, BlitMultiTap and CopyTexture overloads that cannot be recorded throw NotSupportedException naming the overload");
            ShaderSourceComesFromThePlan();
            Console.WriteLine("PASS: a planned shader keeps the file and path id of its programs and sends them with the shader");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static Shader NewShader(string name) => DisunityHost.RegisterShader(name);

    private static byte[] Utf8(string text) => Encoding.UTF8.GetBytes(text);

    private static DisunityPropertyRead Read(uint scope, ulong target, string name)
    {
        var bytes = Utf8(name);
        DisunityPropertyRead value;
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_shader_property_read(DisunityHost.WorldHandle, scope, target, pointer, (uint)bytes.Length, &value));
        return value;
    }

    private static bool KeywordOn(uint scope, ulong target, string name)
    {
        var bytes = Utf8(name);
        uint on;
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_shader_keyword_read(DisunityHost.WorldHandle, scope, target, pointer, (uint)bytes.Length, &on));
        return on == 1;
    }

    private static DisunityMaterialInfo Info(ulong material)
    {
        DisunityMaterialInfo info;
        DisunityHost.ThrowIfError(NativeBindings.disunity_material_read(DisunityHost.WorldHandle, material, &info));
        return info;
    }

    private static DisunityRenderTexture ReadTexture(ulong id)
    {
        DisunityRenderTexture desc;
        DisunityHost.ThrowIfError(NativeBindings.disunity_render_texture_read(DisunityHost.WorldHandle, id, &desc));
        return desc;
    }

    /// <summary>Drains the world's offscreen work.</summary>
    private static DisunityOffscreenWork[] Take()
    {
        uint count;
        DisunityHost.ThrowIfError(NativeBindings.disunity_offscreen_take(DisunityHost.WorldHandle, null, 0, &count));
        var work = new DisunityOffscreenWork[count];
        fixed (DisunityOffscreenWork* pointer = work)
            DisunityHost.ThrowIfError(NativeBindings.disunity_offscreen_take(DisunityHost.WorldHandle, count == 0 ? null : pointer, count, &count));
        return work;
    }

    private static void MaterialSettersReachTheWorld()
    {
        var material = new Material(NewShader("Test/World/Setters"));
        var link = material.WorldLink;
        material.SetFloat("_Cutoff", 0.25f);
        material.SetInteger("_Mode", -3);
        material.SetColor("_Tint", new Color(0.5f, 0.25f, 0.125f, 1f));
        material.SetVector("_Vec", new Vector4(1, 2, 3, 4));
        var plain = new Texture2D(4, 4);
        material.SetTexture("_Main", plain);
        material.SetTextureScale("_Main", new Vector2(2, 3));
        material.SetTextureOffset("_Main", new Vector2(0.5f, 0.25f));
        material.EnableKeyword("FOG");
        material.EnableKeyword("RAIN");
        material.DisableKeyword("RAIN");
        material.renderQueue = 2450;
        material.SetShaderPassEnabled("ShadowCaster", false);

        Expect.Near(0.25f, Read(Material, link.Id, "_Cutoff").float_value, "_Cutoff");
        Expect.Equal(-3, Read(Material, link.Id, "_Mode").int_value, "_Mode");
        var tint = Read(Material, link.Id, "_Tint");
        Expect.That(tint.present == 4, "a color is a vector property");
        Expect.Near(0.5f, tint.vector_x, "_Tint.r");
        Expect.Near(0.125f, tint.vector_z, "_Tint.b");
        Expect.Near(4f, Read(Material, link.Id, "_Vec").vector_w, "_Vec.w");
        var main = Read(Material, link.Id, "_Main");
        Expect.That(main.present == 16 && main.texture_kind == 1 && main.texture == plain.RendererIdentity, "_Main names the texture's renderer identity");
        Expect.Near(2f, main.scale_x, "_Main scale x");
        Expect.Near(3f, main.scale_y, "_Main scale y");
        Expect.Near(0.5f, main.offset_x, "_Main offset x");
        Expect.Near(0.25f, main.offset_y, "_Main offset y");
        Expect.That(KeywordOn(Material, link.Id, "FOG") && !KeywordOn(Material, link.Id, "RAIN"), "keywords");
        var info = Info(link.Id);
        Expect.That(info.render_queue == 2450 && info.keyword_count == 1 && info.disabled_pass_count == 1, "queue, keyword count and disabled passes");
        Expect.That(info.generation > 8, "every edit raised the generation");

        var target = new RenderTexture(8, 8, 0);
        material.SetTexture("_Splat", target);
        var splat = Read(Material, link.Id, "_Splat");
        Expect.That(splat.texture_kind == 2 && splat.texture == target.WorldId, "a render texture property names the world render texture");
        material.SetTexture("_Splat", null!);
        Expect.That(Read(Material, link.Id, "_Splat").texture_kind == 0, "a null texture is no texture");
    }

    private static void MaterialStateBeforeItsFirstUseIsSentWhole()
    {
        var material = new Material(NewShader("Test/World/Whole"));
        material.SetFloat("_Early", 9f);
        material.EnableKeyword("EARLY");
        material.renderQueue = 3100;
        material.SetShaderPassEnabled("Meta", false);
        material.SetTextureScale("_Tex", new Vector2(4, 5));
        var link = material.WorldLink;
        Expect.Near(9f, Read(Material, link.Id, "_Early").float_value, "_Early");
        Expect.That(KeywordOn(Material, link.Id, "EARLY"), "EARLY");
        var info = Info(link.Id);
        Expect.That(info.render_queue == 3100 && info.disabled_pass_count == 1, "queue and disabled passes");
        Expect.Near(4f, Read(Material, link.Id, "_Tex").scale_x, "a scale set before a texture");
        Expect.That(material.WorldLink.Id == link.Id, "the world material is made once");
    }

    private static void MaterialCopiesAndShaderChangesFollow()
    {
        var first = NewShader("Test/World/First");
        var second = NewShader("Test/World/Second");
        var material = new Material(first);
        var link = material.WorldLink;
        material.SetFloat("_A", 1f);
        material.shader = second;
        Expect.That(Info(link.Id).shader == second.WorldLink.Id, "the world material uses the new shader");

        var source = new Material(second);
        source.SetFloat("_B", 2f);
        source.EnableKeyword("COPIED");
        source.renderQueue = 2600;
        material.CopyPropertiesFromMaterial(source);
        Expect.That(Read(Material, link.Id, "_A").present == 0, "a copy removes the properties it does not have");
        Expect.Near(2f, Read(Material, link.Id, "_B").float_value, "_B");
        Expect.That(KeywordOn(Material, link.Id, "COPIED") && Info(link.Id).render_queue == 2600, "keywords and queue are copied");

        material.shaderKeywords = new[] { "ONLY" };
        Expect.That(KeywordOn(Material, link.Id, "ONLY") && !KeywordOn(Material, link.Id, "COPIED"), "shaderKeywords replaces the keywords");

        var duplicate = new Material(material);
        var duplicateId = duplicate.WorldLink.Id;
        Expect.That(duplicateId != link.Id, "a copy has a world material of its own");
        Expect.Near(2f, Read(Material, duplicateId, "_B").float_value, "the copy's properties");

        UnityEngine.Object.DestroyImmediate(duplicate);
        Expect.Throws<DisunityException>(() => Info(duplicateId), "a destroyed material's world material is gone");
    }

    private static void GlobalsReachTheWorld()
    {
        DisunityHost.UnloadWorld();
        Shader.SetGlobalFloat("_EarlyGlobal", 6f);
        Shader.EnableKeyword("EARLY_GLOBAL");
        DisunityHost.LoadWorld();
        Expect.Near(6f, Read(Global, 0, "_EarlyGlobal").float_value, "a global set before the world was loaded");
        Expect.That(KeywordOn(Global, 0, "EARLY_GLOBAL"), "a global keyword set before the world was loaded");

        Shader.SetGlobalVector("_GlobalVec", new Vector4(1, 2, 3, 4));
        Shader.SetGlobalColor("_GlobalColor", new Color(0.1f, 0.2f, 0.3f, 0.4f));
        Shader.SetGlobalInteger("_GlobalInt", 8);
        Shader.SetGlobalMatrix("_GlobalMatrix", Matrix4x4.identity);
        var target = new RenderTexture(4, 4, 0);
        Shader.SetGlobalTexture("_GlobalTex", target);
        Shader.DisableKeyword("EARLY_GLOBAL");
        Shader.EnableKeyword("LATE_GLOBAL");
        Expect.Near(3f, Read(Global, 0, "_GlobalVec").vector_z, "_GlobalVec.z");
        Expect.Near(0.3f, Read(Global, 0, "_GlobalColor").vector_z, "_GlobalColor.b");
        Expect.Equal(8, Read(Global, 0, "_GlobalInt").int_value, "_GlobalInt");
        Expect.That(Read(Global, 0, "_GlobalMatrix").present == 8, "_GlobalMatrix");
        var texture = Read(Global, 0, "_GlobalTex");
        Expect.That(texture.texture_kind == 2 && texture.texture == target.WorldId, "a global texture names the world render texture");
        Expect.That(!KeywordOn(Global, 0, "EARLY_GLOBAL") && KeywordOn(Global, 0, "LATE_GLOBAL"), "global keywords follow");
        Shader.DisableKeyword("LATE_GLOBAL");
    }

    private static void RenderTexturesReachTheWorld()
    {
        var texture = new RenderTexture(64, 32, 24, GraphicsFormat.R8G8B8A8_SRGB, 3);
        Expect.That(texture.Create(), "Create");
        var id = texture.WorldId;
        var desc = ReadTexture(id);
        Expect.That(desc.width == 64 && desc.height == 32 && desc.depth_bits == 24 && desc.mip_count == 3 && desc.srgb == 1, "the description");
        Expect.That(desc.format == (uint)GraphicsFormat.R8G8B8A8_SRGB, "the graphics format");

        texture.Release();
        texture.width = 128;
        texture.Create();
        Expect.That(texture.WorldId == id, "Release and Create keep the world id");
        var resized = ReadTexture(id);
        Expect.That(resized.width == 128 && resized.generation > desc.generation, "Create sends the new size");

        var shadow = new RenderTexture(4, 4, 16);
        Expect.That(ReadTexture(shadow.WorldId).depth_bits == 16, "16 depth bits");
        var flat = new RenderTexture(4, 4, 0);
        Expect.That(ReadTexture(flat.WorldId).depth_bits == 0, "no depth bits");

        UnityEngine.Object.DestroyImmediate(texture);
        Expect.Throws<DisunityException>(() => ReadTexture(id), "a destroyed texture's world texture");
    }

    private static MeshRenderer NewMeshRenderer() => DisunityHost.CreateGameObject().AddComponent<MeshRenderer>();

    private static void RendererSlotsAndBlocksReachTheWorld()
    {
        var renderer = NewMeshRenderer();
        var first = new Material(NewShader("Test/World/Slot"));
        var second = new Material(NewShader("Test/World/Slot"));
        renderer.sharedMaterials = new[] { first, null!, second };
        uint count;
        var ids = new ulong[3];
        fixed (ulong* pointer = ids)
            DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_materials_read(DisunityHost.WorldHandle, renderer.Handle, pointer, 3, &count));
        Expect.That(count == 3 && ids[0] == first.WorldLink.Id && ids[1] == 0 && ids[2] == second.WorldLink.Id, "the slots name their world materials");

        var block = new MaterialPropertyBlock();
        block.SetFloat("_Fade", 0.75f);
        renderer.SetPropertyBlock(block);
        Expect.Near(0.75f, Read(Block, renderer.Handle, "_Fade").float_value, "the property block");
        renderer.SetPropertyBlock(null);
        Expect.That(Read(Block, renderer.Handle, "_Fade").present == 0, "removing the block");
        var other = new MaterialPropertyBlock();
        other.SetFloat("_Single", 1f);
        renderer.SetPropertyBlock(other, 0);
        Expect.That(Read(Block, renderer.Handle, "_Single").present == 0, "a block for one material index stays in managed memory");
    }

    private static Camera NewCamera() => DisunityHost.CreateGameObject().AddComponent<Camera>();

    private static void CameraRenderRecordsOffscreenWork()
    {
        Take();
        var camera = NewCamera();
        var target = new RenderTexture(16, 16, 0);
        camera.targetTexture = target;
        Expect.That(camera.targetTexture == target, "targetTexture reads back");
        camera.Render();
        camera.Render();
        var work = Take();
        Expect.That(work.Length == 2 && work.All(w => w.kind == 0 && w.camera == camera.Handle && w.target == target.WorldId), "two camera renders");
        Expect.That(target.IsCreated(), "Render creates the target");

        var screen = NewCamera();
        Expect.Throws<NotSupportedException>(() => screen.Render(), "a camera without a target");
        Expect.That(Take().Length == 0, "a camera without a target records nothing");
    }

    private static void BlitAndCopyRecordInOrder()
    {
        Take();
        var source = new Texture2D(4, 4);
        var a = new RenderTexture(8, 8, 0, GraphicsFormat.R8G8B8A8_UNorm, 3);
        var b = new RenderTexture(8, 8, 0);
        var c = new RenderTexture(8, 8, 0, GraphicsFormat.R8G8B8A8_UNorm, 3);
        var material = new Material(DisunityHost.DeclareShader("Test/World/Blit", 2000, Unnamed(3), 1, _ => { }));
        Graphics.Blit(source, a);
        Graphics.Blit(a, b, material, 2);
        RenderTexture.active = b;
        Graphics.Blit(null!, material);
        Graphics.CopyTexture(b, 0, 0, a, 0, 2);
        Graphics.CopyTexture(a, c);
        var work = Take();

        Expect.That(work[0].kind == 1 && work[0].source_kind == 1 && work[0].source == source.RendererIdentity && work[0].target == a.WorldId
            && work[0].material == 0 && work[0].pass == -1, "Blit(Texture2D, RenderTexture)");
        Expect.That(work[1].kind == 1 && work[1].source_kind == 2 && work[1].source == a.WorldId && work[1].target == b.WorldId
            && work[1].material == material.WorldLink.Id && work[1].pass == 2, "Blit(RenderTexture, RenderTexture, Material, int)");
        Expect.That(work[2].kind == 1 && work[2].source_kind == 0 && work[2].target == b.WorldId && work[2].pass == -1
            && work[2].material == material.WorldLink.Id, "Blit(null, Material) draws into the active render texture");
        Expect.That(work[3].kind == 2 && work[3].source == b.WorldId && work[3].target == a.WorldId && work[3].source_mip == 0 && work[3].target_mip == 2,
            "CopyTexture with elements and mips");
        Expect.That(work.Length == 4 + 3, "a whole copy records one copy per shared mip level: " + work.Length);
        for (uint mip = 0; mip < 3; mip++)
        {
            var copy = work[4 + (int)mip];
            Expect.That(copy.kind == 2 && copy.source == a.WorldId && copy.target == c.WorldId && copy.source_mip == mip && copy.target_mip == mip,
                $"whole copy, mip {mip}");
        }
        RenderTexture.active = null;
    }

    private static void UnsupportedOverloadsThrowNamingTheMember()
    {
        var source = new Texture2D(2, 2);
        var target = new RenderTexture(2, 2, 0);
        var material = new Material(NewShader("Test/World/Unsupported"));
        RenderTexture.active = null;
        NotSupported(() => Graphics.Blit(source, (RenderTexture)null!), "Blit to the screen", "Graphics.Blit(Texture, RenderTexture)");
        NotSupported(() => Graphics.Blit(source, material), "Blit with no active texture", "Graphics.Blit(Texture, Material, int)");
        NotSupported(() => Graphics.Blit(source, target, new Vector2(2, 2), Vector2.zero), "Blit with a scale", "Graphics.Blit(Texture, RenderTexture, Vector2, Vector2)");
        NotSupported(() => Graphics.Blit(source, target, material, 0, 1), "Blit to a depth slice", "Graphics.Blit(Texture, RenderTexture, Material, int, int)");
        NotSupported(() => Graphics.BlitMultiTap(source, target, material, Vector2.zero), "BlitMultiTap", "Graphics.BlitMultiTap");
        NotSupported(() => Graphics.CopyTexture(target, 0, 0, 0, 0, 1, 1, target, 0, 0, 0, 0), "a region copy", "Graphics.CopyTexture(Texture, int, int, int, int, int, int, Texture, int, int, int, int)");
        NotSupported(() => Graphics.CopyTexture(target, new Texture2D(2, 2)), "a copy into a Texture2D", "Graphics.CopyTexture(Texture, Texture)");
        Take();
    }

    private static void ShaderSourceComesFromThePlan()
    {
        var shader = DisunityHost.DeclareShader("Test/World/Planned", 2000, Unnamed(1), 1, _ => { }, "sharedassets3.assets", -12);
        Expect.That(shader.SourceFile == "sharedassets3.assets" && shader.SourcePathId == -12, "the shader keeps its source");
        var again = DisunityHost.DeclareShader("Test/World/Planned", 2000, Unnamed(1), 1, _ => { }, "other.assets", 5);
        Expect.That(again == shader && shader.SourceFile == "sharedassets3.assets", "the first source stays");
        Expect.That(shader.WorldLink.Id != 0, "the shader is registered in the world with its source");
        var plain = DisunityHost.RegisterShader("Test/World/Fallback");
        Expect.That(plain.SourceFile is null && plain.WorldLink.Id != 0, "a shader made by name has no source");
    }
}
