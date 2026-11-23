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
    private const uint Material = 0, Global = 1, Block = 2, SlotBlock = 4;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            MaterialSettersReachTheWorld();
            Console.WriteLine("PASS: a material's float, integer, color, vector, texture, offset, scale, keyword, queue and pass setters reach its world material");
            NamedTexturesTheWorldCannotHoldAreSentAsNotSent();
            Console.WriteLine("PASS: a destroyed or empty texture set on a material is sent as named and not sent, and null stays no texture");
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
            SlotBlocksReachTheWorldPerMaterialIndex();
            Console.WriteLine("PASS: a property block set for one material index reaches that slot's block in the world, and clearing it removes it");
            SlotBlocksReachTheWorldUpToTheLastSlot();
            Console.WriteLine("PASS: a property block reaches its own slot up to index 63, and index 64 stays in managed memory");
            CameraRenderRecordsOffscreenWork();
            Console.WriteLine("PASS: Camera.Render with a target texture records CameraRender, and without one records nothing");
            BlitAndCopyRecordInOrder();
            Console.WriteLine("PASS: Graphics.Blit and Graphics.CopyTexture record offscreen work in call order");
            ABlitKeepsTheMaterialItSawWhenTheScriptEditsItLater();
            Console.WriteLine("PASS: a texture set on a material after a Graphics.Blit leaves that blit with the material it saw");
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

    internal static DisunityPropertyRead Read(uint scope, ulong target, string name)
    {
        var bytes = Utf8(name);
        DisunityPropertyRead value;
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_shader_property_read(DisunityHost.WorldHandle, scope, target, pointer, (uint)bytes.Length, &value));
        return value;
    }

    internal static bool KeywordOn(uint scope, ulong target, string name)
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

    private static void NamedTexturesTheWorldCannotHoldAreSentAsNotSent()
    {
        const uint none = 0, unsent = 5;
        var material = new Material(NewShader("Test/World/Unsent"));
        var link = material.WorldLink;
        var destroyed = new RenderTexture(4, 4, 0);
        UnityEngine.Object.DestroyImmediate(destroyed);
        material.SetTexture("_Destroyed", destroyed);
        material.SetTexture("_Empty", new Texture2D(0, 0));
        material.SetTexture("_Null", null!);
        var held = new (string Property, uint Kind)[] { ("_Destroyed", unsent), ("_Empty", unsent), ("_Null", none) };
        foreach (var (property, kind) in held)
        {
            var slot = Read(Material, link.Id, property);
            Expect.That(slot.present == 16 && slot.texture_kind == kind && slot.texture == 0, $"{property} is sent as kind {kind}, got {slot.texture_kind}");
        }
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
        Expect.Near(1f, Read(SlotBlock, SlotTarget(renderer, 0), "_Single").float_value, "a block for one material index reaches that slot's block");
        Expect.That(Read(Block, renderer.Handle, "_Single").present == 0, "a block for one material index is not the all-material block");
        Expect.That(Read(SlotBlock, SlotTarget(renderer, 1), "_Single").present == 0, "and not another slot's block");
        Expect.That(!renderer.HasPropertyBlock(), "a block for one material index is not the block of every material");
    }

    private static ulong SlotTarget(Renderer renderer, uint index)
    {
        ulong target;
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_slot_block_target(DisunityHost.WorldHandle, renderer.Handle, index, &target));
        return target;
    }

    private static void SlotBlocksReachTheWorldPerMaterialIndex()
    {
        var renderer = NewMeshRenderer();
        renderer.sharedMaterials = new[] { new Material(NewShader("Test/World/Slot")), new Material(NewShader("Test/World/Slot")) };
        var texture = new Texture2D(4, 4);
        var slotBlock = new MaterialPropertyBlock();
        slotBlock.SetTexture("_MainTex", texture);
        slotBlock.SetFloat("_Fade", 0.25f);
        renderer.SetPropertyBlock(slotBlock, 1);
        var second = Read(SlotBlock, SlotTarget(renderer, 1), "_MainTex");
        Expect.That(second.present == 16 && second.texture_kind == 1 && second.texture == texture.RendererIdentity, "the block of material 1 names its texture");
        Expect.That(Read(SlotBlock, SlotTarget(renderer, 0), "_MainTex").present == 0, "the block of material 0 does not");
        Expect.That(Read(Block, renderer.Handle, "_MainTex").present == 0, "the block of every material does not");

        var every = new MaterialPropertyBlock();
        every.SetFloat("_Fade", 0.75f);
        renderer.SetPropertyBlock(every);
        Expect.Near(0.25f, Read(SlotBlock, SlotTarget(renderer, 1), "_Fade").float_value, "the block of material 1 keeps its own value beside the block of every material");
        Expect.Near(0.75f, Read(Block, renderer.Handle, "_Fade").float_value, "the block of every material is its own");

        var replacement = new MaterialPropertyBlock();
        replacement.SetFloat("_Other", 2f);
        renderer.SetPropertyBlock(replacement, 1);
        Expect.That(Read(SlotBlock, SlotTarget(renderer, 1), "_MainTex").present == 0, "setting a block for a material replaces the earlier one");
        Expect.Near(2f, Read(SlotBlock, SlotTarget(renderer, 1), "_Other").float_value, "the replacement reaches the world");

        renderer.SetPropertyBlock(null, 1);
        Expect.That(Read(SlotBlock, SlotTarget(renderer, 1), "_Other").present == 0, "clearing the block of a material removes it from the world");
        Expect.Near(0.75f, Read(Block, renderer.Handle, "_Fade").float_value, "and keeps the block of every material");

        renderer.SetPropertyBlock(slotBlock, 200);
        var held = new MaterialPropertyBlock();
        renderer.GetPropertyBlock(held, 200);
        Expect.That(held.HasFloat("_Fade"), "a block for an index no slot can hold is kept and read back");
    }

    private static void SlotBlocksReachTheWorldUpToTheLastSlot()
    {
        var renderer = NewMeshRenderer();
        var block = new MaterialPropertyBlock();
        block.SetFloat("_Fade", 0.5f);
        renderer.SetPropertyBlock(block, 2);
        renderer.SetPropertyBlock(block, 63);
        Expect.Near(0.5f, Read(SlotBlock, SlotTarget(renderer, 2), "_Fade").float_value, "the block of material 2 reaches the world under its own index");
        Expect.Near(0.5f, Read(SlotBlock, SlotTarget(renderer, 63), "_Fade").float_value, "the block of the last slot, 63, reaches the world");
        Expect.That(Read(SlotBlock, SlotTarget(renderer, 1), "_Fade").present == 0, "and no other slot holds it");

        renderer.SetPropertyBlock(block, 64);
        var held = new MaterialPropertyBlock();
        renderer.GetPropertyBlock(held, 64);
        Expect.That(held.HasFloat("_Fade"), "the first index past the last slot is kept in managed memory and read back");
    }

    private static Camera NewCamera() => DisunityHost.CreateGameObject().AddComponent<Camera>();

    private static void CameraRenderRecordsOffscreenWork()
    {
        CommandWireReader.Work(DisunityHost.WorldHandle);
        var camera = NewCamera();
        var target = new RenderTexture(16, 16, 0);
        camera.targetTexture = target;
        Expect.That(camera.targetTexture == target, "targetTexture reads back");
        var other = NewCamera();
        var otherTarget = new RenderTexture(8, 8, 0);
        other.targetTexture = otherTarget;
        camera.Render();
        other.Render();
        camera.Render();
        var work = CommandWireReader.Work(DisunityHost.WorldHandle);
        Expect.That(work.Length == 3 && work.All(w => w.Tag == 0), "three camera renders");
        Expect.That(work[0].Target == target.WorldId && work[1].Target == otherTarget.WorldId && work[2].Target == target.WorldId, "each render names its camera's target");
        Expect.That(work[0].Camera == work[2].Camera && work[0].Camera != work[1].Camera, "a camera render names the component of the camera that rendered");
        Expect.That(target.IsCreated(), "Render creates the target");

        var screen = NewCamera();
        Expect.Throws<NotSupportedException>(() => screen.Render(), "a camera without a target");
        Expect.That(CommandWireReader.Work(DisunityHost.WorldHandle).Length == 0, "a camera without a target records nothing");
    }

    private static void BlitAndCopyRecordInOrder()
    {
        CommandWireReader.Work(DisunityHost.WorldHandle);
        var source = new Texture2D(4, 4);
        var a = new RenderTexture(8, 8, 0, GraphicsFormat.R8G8B8A8_UNorm, 3);
        var b = new RenderTexture(8, 8, 0);
        var c = new RenderTexture(8, 8, 0, GraphicsFormat.R8G8B8A8_UNorm, 3);
        var material = new Material(DisunityHost.DeclareShader("Test/World/Blit", new ShaderDeclaration(2000, 3, 1, Unnamed(3)), _ => { }));
        Graphics.Blit(source, a);
        Graphics.Blit(a, b, material, 2);
        RenderTexture.active = b;
        Graphics.Blit(null!, material);
        Graphics.CopyTexture(b, 0, 0, a, 0, 2);
        Graphics.CopyTexture(a, c);
        var work = CommandWireReader.Work(DisunityHost.WorldHandle);

        Expect.That(work[0].Tag == 1 && work[0].SourceKind == 1 && work[0].Source == source.RendererIdentity && work[0].Target == a.WorldId
            && work[0].Material == 0 && work[0].Pass == -1, "Blit(Texture2D, RenderTexture)");
        Expect.That(work[1].Tag == 1 && work[1].SourceKind == 2 && work[1].Source == a.WorldId && work[1].Target == b.WorldId
            && work[1].Material == material.WorldLink.Id && work[1].Pass == 2, "Blit(RenderTexture, RenderTexture, Material, int)");
        Expect.That(work[2].Tag == 1 && work[2].SourceKind == 0 && work[2].Target == b.WorldId && work[2].Pass == -1
            && work[2].Material == material.WorldLink.Id, "Blit(null, Material) draws into the active render texture");
        Expect.That(work[3].Tag == 2 && work[3].Source == b.WorldId && work[3].Target == a.WorldId && work[3].SourceMip == 0 && work[3].TargetMip == 2,
            "CopyTexture with elements and mips");
        Expect.That(work.Length == 4 + 3, "a whole copy records one copy per shared mip level: " + work.Length);
        for (uint mip = 0; mip < 3; mip++)
        {
            var copy = work[4 + (int)mip];
            Expect.That(copy.Tag == 2 && copy.Source == a.WorldId && copy.Target == c.WorldId && copy.SourceMip == mip && copy.TargetMip == mip,
                $"whole copy, mip {mip}");
        }
        RenderTexture.active = null;
    }

    /// <summary>The bake pattern of a terrain: set a texture, blit, set another texture, blit. A record names the material by its
    /// world id while the material is as the blit saw it; the first edit afterwards moves the earlier blit to a copy of that state,
    /// an id of generation 0.</summary>
    private static void ABlitKeepsTheMaterialItSawWhenTheScriptEditsItLater()
    {
        CommandWireReader.Work(DisunityHost.WorldHandle);
        var first = new RenderTexture(8, 8, 0);
        var second = new RenderTexture(8, 8, 0);
        var target = new RenderTexture(8, 8, 0);
        var material = new Material(DisunityHost.DeclareShader("Test/World/Bake", new ShaderDeclaration(2000, 1, 1, Unnamed(1)), _ => { }));
        material.SetTexture("_PrimaryTex", first);
        Graphics.Blit(first, target, material, 0);
        material.SetTexture("_PrimaryTex", second);
        Graphics.Blit(second, target, material, 0);
        var work = CommandWireReader.Work(DisunityHost.WorldHandle);

        Expect.That(work.Length == 2, "two blits: " + work.Length);
        Expect.That(work[1].Material == material.WorldLink.Id, "the last blit names the material itself");
        Expect.That(work[0].Material != work[1].Material && work[0].Material >> 32 == 0,
            "the blit before the edit names a copy of the material (generation 0): " + work[0].Material);
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
        CommandWireReader.Work(DisunityHost.WorldHandle);
    }

    private static void ShaderSourceComesFromThePlan()
    {
        var shader = DisunityHost.DeclareShader("Test/World/Planned", new ShaderDeclaration(2000, 1, 1, Unnamed(1)), _ => { }, "sharedassets3.assets", -12);
        Expect.That(shader.SourceFile == "sharedassets3.assets" && shader.SourcePathId == -12, "the shader keeps its source");
        var again = DisunityHost.DeclareShader("Test/World/Planned", new ShaderDeclaration(2000, 1, 1, Unnamed(1)), _ => { }, "other.assets", 5);
        Expect.That(again == shader && shader.SourceFile == "sharedassets3.assets", "the first source stays");
        Expect.That(shader.WorldLink.Id != 0, "the shader is registered in the world with its source");
        var plain = DisunityHost.RegisterShader("Test/World/Fallback");
        Expect.That(plain.SourceFile is null && plain.WorldLink.Id != 0, "a shader made by name has no source");
    }
}
