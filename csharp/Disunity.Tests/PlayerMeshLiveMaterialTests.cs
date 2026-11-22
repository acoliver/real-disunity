using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;

/// <summary>The host draws a slot with the colour and texture its world material holds when it prepares a frame, so a
/// material changed after the renderer took it shows without the renderer's materials being set again, and a slot with
/// no material draws white with no texture.</summary>
internal static class PlayerMeshLiveMaterialTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ShowsATextureAndColourSetAfterTheMaterialWasAssigned();
            ShowsTheTextureSetThroughSetTexture();
            ShowsTheCopyARendererMaterialGives();
            ShowsBaseColorWhenThereIsNoColor();
            ShowsTheChangeOnEveryRendererThatSharesTheMaterial();
            DrawsAColourThatIsNotUsableAsZero();
            DrawsAMaterialWithoutDrawableTexturesUntextured();
            DrawsOnlyTheSlotsTheMaterialsHold();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    /// <summary>A mesh renderer on an object whose filter holds a mesh, so a prepared frame lists it.</summary>
    internal static MeshRenderer NewRenderer()
    {
        var shown = new GameObject("live material");
        shown.AddComponent<MeshFilter>().sharedMesh = new Mesh { vertices = [Vector3.zero, Vector3.right, Vector3.up], triangles = [0, 1, 2] };
        return shown.AddComponent<MeshRenderer>();
    }

    private static Material NewMaterial() => new(DisunityHost.RegisterShader("Test/PlayerMesh/Plain"));

    /// <summary>The slots of the mesh object the next presented frame lists for <paramref name="renderer"/>.</summary>
    internal static unsafe DisunityDrawnMaterial[] Prepared(Renderer renderer)
    {
        var world = DisunityHost.WorldHandle;
        var header = new DisunityPresentation();
        DisunityHost.ThrowIfError(NativeBindings.disunity_presentation_prepare(world, 80, 40, 18, &header));
        for (uint index = 0; index < header.mesh_objects; index++)
        {
            var record = new DisunityMeshObject();
            var matrix = new DisunityMatrix();
            DisunityHost.ThrowIfError(NativeBindings.disunity_presentation_mesh_object(world, index, &record, &matrix, null, 0));
            if (record.@object != renderer.gameObject.NativeHandle) continue;
            var slots = new DisunityDrawnMaterial[record.slot_count];
            fixed (DisunityDrawnMaterial* pointer = slots)
                DisunityHost.ThrowIfError(NativeBindings.disunity_presentation_mesh_object(world, index, &record, &matrix, pointer, record.slot_count));
            return slots;
        }
        throw new InvalidOperationException("the frame does not list the renderer");
    }

    private static bool Near(float actual, float expected) => Math.Abs(actual - expected) < 1e-6f;

    private static bool Shows(DisunityDrawnMaterial slot, Color color, ulong texture) =>
        Near(slot.r, color.r) && Near(slot.g, color.g) && Near(slot.b, color.b) && Near(slot.a, color.a) && slot.texture == texture;

    private static void ShowsATextureAndColourSetAfterTheMaterialWasAssigned()
    {
        var renderer = NewRenderer();
        var material = NewMaterial();
        renderer.sharedMaterial = material;
        Expect.That(Shows(Prepared(renderer)[0], Color.white, 0), "a material with nothing set draws white and untextured");
        var texture = new Texture2D(2, 2);
        material.mainTexture = texture;
        material.color = new Color(0.2f, 0.4f, 0.6f, 0.8f);
        Expect.That(Shows(Prepared(renderer)[0], new Color(0.2f, 0.4f, 0.6f, 0.8f), texture.RendererIdentity), "a texture and colour set after the material was assigned are drawn");
        var other = new Texture2D(1, 1);
        material.mainTexture = other;
        material.color = Color.red;
        Expect.That(Shows(Prepared(renderer)[0], Color.red, other.RendererIdentity), "a second change is drawn too");
        material.mainTexture = null!;
        Expect.That(Prepared(renderer)[0].texture == 0, "removing the texture draws untextured");
        Console.WriteLine("PASS: a material changed after it was assigned draws with its current colour and texture");
    }

    private static void ShowsTheTextureSetThroughSetTexture()
    {
        var renderer = NewRenderer();
        var material = NewMaterial();
        renderer.sharedMaterial = material;
        var texture = new Texture2D(4, 4);
        material.SetTexture("_MainTex", texture);
        Expect.That(Prepared(renderer)[0].texture == texture.RendererIdentity, "SetTexture on _MainTex is the main texture");
        material.SetTexture("_BumpMap", new Texture2D(2, 2));
        Expect.That(Prepared(renderer)[0].texture == texture.RendererIdentity, "another texture name does not replace it");
        Console.WriteLine("PASS: SetTexture on _MainTex is drawn");
    }

    private static void ShowsTheCopyARendererMaterialGives()
    {
        var renderer = NewRenderer();
        var source = NewMaterial();
        renderer.sharedMaterial = source;
        var copy = renderer.material!;
        var texture = new Texture2D(2, 2);
        copy.mainTexture = texture;
        copy.color = Color.green;
        source.color = Color.blue;
        Expect.That(Shows(Prepared(renderer)[0], Color.green, texture.RendererIdentity), "the renderer draws its own copy, not the material it was copied from");
        Console.WriteLine("PASS: renderer.material changes are drawn and its source's are not");
    }

    private static void ShowsBaseColorWhenThereIsNoColor()
    {
        var renderer = NewRenderer();
        var material = NewMaterial();
        renderer.sharedMaterial = material;
        material.SetColor("_BaseColor", new Color(0.5f, 0.6f, 0.7f, 1f));
        Expect.That(Shows(Prepared(renderer)[0], new Color(0.5f, 0.6f, 0.7f, 1f), 0), "_BaseColor draws when the material has no _Color");
        material.SetColor("_Color", Color.red);
        Expect.That(Shows(Prepared(renderer)[0], Color.red, 0), "_Color wins when the material has both");
        Console.WriteLine("PASS: _BaseColor is drawn when there is no _Color");
    }

    private static void ShowsTheChangeOnEveryRendererThatSharesTheMaterial()
    {
        var first = NewRenderer();
        var second = NewRenderer();
        var material = NewMaterial();
        first.sharedMaterial = material;
        second.sharedMaterials = [material, null];
        var texture = new Texture2D(2, 2);
        material.mainTexture = texture;
        material.color = Color.green;
        Expect.That(Shows(Prepared(first)[0], Color.green, texture.RendererIdentity), "the first renderer draws the change");
        var shared = Prepared(second);
        Expect.That(Shows(shared[0], Color.green, texture.RendererIdentity), "the second renderer draws the change");
        Expect.That(shared[1].texture == 0 && shared[1].r == 1, "an empty slot stays white and untextured");
        Console.WriteLine("PASS: a shared material changes every renderer that holds it");
    }

    private static void DrawsAColourThatIsNotUsableAsZero()
    {
        var renderer = NewRenderer();
        var material = NewMaterial();
        renderer.sharedMaterial = material;
        material.color = new Color(-1f, float.NaN, 2f, 1f);
        var slot = Prepared(renderer)[0];
        Expect.That(slot.r == 0 && slot.g == 0 && slot.b == 2 && slot.a == 1, "a negative channel and one that is not a number are drawn as 0 after a later change");
        Console.WriteLine("PASS: a later colour change with unusable channels draws them as 0");
    }

    private static void DrawsAMaterialWithoutDrawableTexturesUntextured()
    {
        var renderer = NewRenderer();
        var wide = NewMaterial();
        wide.mainTexture = new Texture2D(2, 0);
        var tall = NewMaterial();
        tall.mainTexture = new Texture2D(0, 2);
        var target = NewMaterial();
        target.mainTexture = new RenderTexture(4, 4, 0);
        var whole = NewMaterial();
        var one = new Texture2D(1, 1);
        whole.mainTexture = one;
        renderer.sharedMaterials = [wide, tall, target, whole];
        var slots = Prepared(renderer);
        Expect.That(slots[0].texture == 0 && slots[1].texture == 0, "a texture with no width or no height has no pixels to draw");
        Expect.That(slots[2].texture == 0, "a render texture is not a texture with pixels");
        Expect.That(slots[3].texture == one.RendererIdentity, "a one pixel texture is drawn");
        Console.WriteLine("PASS: only a Texture2D with pixels is drawn as a slot's texture");
    }

    private static void DrawsOnlyTheSlotsTheMaterialsHold()
    {
        var renderer = NewRenderer();
        var red = NewMaterial();
        red.color = Color.red;
        var blue = NewMaterial();
        blue.color = Color.blue;
        renderer.sharedMaterials = [red, null, blue];
        var slots = Prepared(renderer);
        Expect.That(slots.Length == 3, "one slot per material");
        Expect.That(Shows(slots[0], Color.red, 0) && Shows(slots[2], Color.blue, 0), "a slot draws its material's colour");
        Expect.That(Shows(slots[1], Color.white, 0), "an empty slot draws white and untextured");
        renderer.sharedMaterials = [blue];
        var fewer = Prepared(renderer);
        Expect.That(fewer.Length == 1 && Shows(fewer[0], Color.blue, 0), "setting fewer materials replaces the slots");
        renderer.sharedMaterials = [];
        Expect.That(Prepared(renderer).Length == 0, "no materials, no slots");
        Console.WriteLine("PASS: a slot draws from its world material and an empty slot draws white");
    }
}
