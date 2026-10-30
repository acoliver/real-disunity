using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;
using EngineObject = UnityEngine.Object;

/// <summary>The native CanvasRenderer component: defaults, color and alpha, clipping, the mesh, texture and
/// material references, counts, inherited alpha, cloning, the loader and script fields.</summary>
internal static class CanvasRendererTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Defaults();
            Console.WriteLine("PASS: CanvasRenderer defaults and fixed members");
            ColorAndAlpha();
            Console.WriteLine("PASS: CanvasRenderer color and alpha are separate and a NaN alpha throws");
            Flags();
            Console.WriteLine("PASS: CanvasRenderer cull, cullTransparentMesh and hasPopInstruction change only themselves");
            Clipping();
            Console.WriteLine("PASS: CanvasRenderer rect clipping and softness");
            MeshAndTextures();
            Console.WriteLine("PASS: CanvasRenderer returns the mesh that was set, takes null and clears");
            Materials();
            Console.WriteLine("PASS: CanvasRenderer materials, pop materials, counts and SetMaterial(Material, Texture)");
            InheritedAlpha();
            Console.WriteLine("PASS: CanvasRenderer.GetInheritedAlpha multiplies the groups above and honors ignoreParentGroups");
            Hierarchy();
            Console.WriteLine("PASS: a CanvasRenderer is a native component, found by type, one per object, and cloned with its own state");
            Loaded();
            Console.WriteLine("PASS: a player plan builds a CanvasRenderer and a script field referencing it binds");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static CanvasRenderer NewRenderer() => DisunityHost.CreateGameObject().AddComponent<CanvasRenderer>();

    private static void Defaults()
    {
        var renderer = NewRenderer();
        Ensure(renderer.GetColor() == Color.white && renderer.GetAlpha() == 1f && renderer.GetInheritedAlpha() == 1f, "white and opaque");
        Ensure(!renderer.cull && !renderer.cullTransparentMesh && !renderer.hasPopInstruction && !renderer.hasMoved, "flags off");
        Ensure(!renderer.hasRectClipping && renderer.clippingSoftness == Vector2.zero, "no clipping");
        Ensure(renderer.materialCount == 0 && renderer.popMaterialCount == 0, "no materials");
        Ensure(renderer.GetMesh() is null && renderer.GetMaterial() is null && renderer.GetMaterial(0) is null && renderer.GetPopMaterial(0) is null, "nothing assigned");
        Ensure(renderer.absoluteDepth == -1 && renderer.relativeDepth == -1, "depths are -1 because nothing batches");
        Ensure(renderer.gameObject.GetComponent<CanvasRenderer>() == renderer, "GetComponent finds it");
        var raised = 0;
        CanvasRenderer.OnRequestRebuild handler = () => raised++;
        CanvasRenderer.onRequestRebuild += handler;
        DisunityHost.Step(0.01f);
        CanvasRenderer.onRequestRebuild -= handler;
        Ensure(raised == 0, "nothing raises onRequestRebuild");
    }

    private static void ColorAndAlpha()
    {
        var renderer = NewRenderer();
        renderer.SetColor(new Color(0.1f, 0.2f, 0.3f, 0.4f));
        Ensure(renderer.GetColor() == new Color(0.1f, 0.2f, 0.3f, 0.4f), "SetColor stores all four channels");
        Ensure(renderer.GetAlpha() == 1f, "SetColor leaves the alpha");
        renderer.SetAlpha(0.5f);
        Ensure(renderer.GetAlpha() == 0.5f && renderer.GetColor().a == 0.4f, "SetAlpha leaves the color's alpha");
        renderer.SetAlpha(7f);
        Ensure(renderer.GetAlpha() == 7f, "an alpha outside 0 through 1 is kept");
        Throws<DisunityException>(() => renderer.SetAlpha(float.NaN), "NaN alpha");
        Ensure(renderer.GetAlpha() == 7f, "a rejected alpha changes nothing");
    }

    private static void Flags()
    {
        var renderer = NewRenderer();
        renderer.cull = true;
        Ensure(renderer.cull && !renderer.cullTransparentMesh && !renderer.hasPopInstruction, "cull alone");
        renderer.cullTransparentMesh = true;
        Ensure(renderer.cull && renderer.cullTransparentMesh && !renderer.hasPopInstruction, "cullTransparentMesh joins");
        renderer.hasPopInstruction = true;
        renderer.cull = false;
        Ensure(!renderer.cull && renderer.cullTransparentMesh && renderer.hasPopInstruction, "clearing cull leaves the others");
    }

    private static void Clipping()
    {
        var renderer = NewRenderer();
        renderer.clippingSoftness = new Vector2(2f, 3f);
        renderer.EnableRectClipping(new Rect(1f, 2f, 30f, 40f));
        Ensure(renderer.hasRectClipping && renderer.clippingSoftness == new Vector2(2f, 3f), "enabled, softness kept");
        renderer.DisableRectClipping();
        Ensure(!renderer.hasRectClipping && renderer.clippingSoftness == new Vector2(2f, 3f), "disabled, softness kept");
        renderer.EnableRectClipping(new Rect(-5f, -6f, 0f, 1f));
        Ensure(renderer.hasRectClipping, "enabled again with an empty-width rect");
    }

    private static void MeshAndTextures()
    {
        var renderer = NewRenderer();
        var mesh = new Mesh();
        var texture = new Texture2D(2, 2);
        var alpha = new Texture2D(2, 2);
        renderer.SetMesh(mesh);
        renderer.SetTexture(texture);
        renderer.SetAlphaTexture(alpha);
        Ensure(ReferenceEquals(renderer.GetMesh(), mesh), "GetMesh returns the same Mesh");
        Ensure(ReferenceEquals(renderer.MainTexture, texture) && ReferenceEquals(renderer.AlphaTexture, alpha), "the textures are kept apart");
        renderer.SetMesh(null);
        Ensure(renderer.GetMesh() is null && renderer.MainTexture is not null, "null clears the mesh only");
        renderer.SetMesh(mesh);
        renderer.materialCount = 1;
        renderer.SetMaterial(new Material(Material.SpriteUnlit), 0);
        Throws<ArgumentException>(() => renderer.SetMesh(destroyed(new Mesh())), "a destroyed mesh", "destroyed");
        Ensure(renderer.GetMesh() == mesh, "a rejected mesh changes nothing");
        renderer.SetColor(new Color(0.5f, 0f, 0f, 1f));
        renderer.Clear();
        Ensure(renderer.GetMesh() is null && renderer.MainTexture is null && renderer.AlphaTexture is null && renderer.materialCount == 0, "Clear drops the mesh, textures and materials");
        Ensure(renderer.GetColor().r == 0.5f, "Clear keeps the color");
        EngineObject.DestroyImmediate(mesh);
        renderer.SetMesh(new Mesh());
        var other = new Mesh();
        renderer.SetMesh(other);
        EngineObject.DestroyImmediate(other);
        Ensure(renderer.GetMesh() is null, "a destroyed mesh is no longer returned");

        static Mesh destroyed(Mesh value) { EngineObject.DestroyImmediate(value); return value; }
    }

    private static void Materials()
    {
        var renderer = NewRenderer();
        var first = new Material(Material.SpriteUnlit);
        var second = new Material(Material.SpriteLit);
        Throws<ArgumentOutOfRangeException>(() => renderer.SetMaterial(first, 0), "a slot before the count is set", "count");
        renderer.materialCount = 2;
        renderer.SetMaterial(first, 0);
        renderer.SetMaterial(second, 1);
        Ensure(ReferenceEquals(renderer.GetMaterial(), first) && ReferenceEquals(renderer.GetMaterial(0), first) && ReferenceEquals(renderer.GetMaterial(1), second), "materials by slot");
        Ensure(renderer.GetMaterial(2) is null && renderer.GetMaterial(-1) is null, "a slot outside the count reads null");
        Throws<ArgumentOutOfRangeException>(() => renderer.SetMaterial(first, 2), "slot 2 of 2", "count");
        renderer.materialCount = 1;
        renderer.materialCount = 2;
        Ensure(renderer.GetMaterial(1) is null && renderer.GetMaterial(0) == first, "lowering the count drops the later slots");
        renderer.materialCount = CanvasRenderer.MaxMaterials;
        renderer.SetMaterial(second, CanvasRenderer.MaxMaterials - 1);
        Throws<ArgumentOutOfRangeException>(() => renderer.materialCount = CanvasRenderer.MaxMaterials + 1, "more than the limit", "count");
        Throws<ArgumentOutOfRangeException>(() => renderer.materialCount = -1, "a negative count", "count");
        Ensure(renderer.materialCount == CanvasRenderer.MaxMaterials, "a rejected count changes nothing");

        renderer.popMaterialCount = 1;
        renderer.SetPopMaterial(first, 0);
        Ensure(ReferenceEquals(renderer.GetPopMaterial(0), first) && renderer.GetPopMaterial(1) is null, "pop materials");
        Throws<ArgumentOutOfRangeException>(() => renderer.SetPopMaterial(first, 1), "pop slot past the count", "count");
        Throws<ArgumentOutOfRangeException>(() => renderer.popMaterialCount = CanvasRenderer.MaxMaterials + 1, "too many pop materials", "count");
        Ensure(ReferenceEquals(renderer.GetMaterial(0), first), "pop materials are separate");

        var texture = new Texture2D(2, 2);
        renderer.SetMaterial(second, texture);
        Ensure(renderer.materialCount == 1 && ReferenceEquals(renderer.GetMaterial(), second) && ReferenceEquals(renderer.MainTexture, texture), "SetMaterial(Material, Texture) sets material 0, count 1 and the texture");
        renderer.SetMaterial(null, 0);
        Ensure(renderer.GetMaterial() is null && renderer.materialCount == 1, "null clears the slot");
    }

    private static void InheritedAlpha()
    {
        var top = DisunityHost.CreateGameObject();
        var middle = DisunityHost.CreateGameObject();
        var leaf = DisunityHost.CreateGameObject();
        middle.transform.SetParent(top.transform, false);
        leaf.transform.SetParent(middle.transform, false);
        var renderer = leaf.AddComponent<CanvasRenderer>();
        renderer.SetAlpha(0.8f);
        Ensure(MathF.Abs(renderer.GetInheritedAlpha() - 0.8f) < 1e-6f, "no groups");
        var topGroup = top.AddComponent<CanvasGroup>();
        var middleGroup = middle.AddComponent<CanvasGroup>();
        topGroup.alpha = 0.5f;
        middleGroup.alpha = 0.5f;
        Ensure(MathF.Abs(renderer.GetInheritedAlpha() - 0.2f) < 1e-6f, $"two nested groups: {renderer.GetInheritedAlpha()}");
        Ensure(renderer.GetAlpha() == 0.8f, "GetAlpha is the renderer's own");
        middleGroup.enabled = false;
        Ensure(MathF.Abs(renderer.GetInheritedAlpha() - 0.4f) < 1e-6f, "a disabled group is skipped");
        middleGroup.enabled = true;
        middleGroup.ignoreParentGroups = true;
        Ensure(MathF.Abs(renderer.GetInheritedAlpha() - 0.4f) < 1e-6f, "ignoreParentGroups stops the walk");
    }

    private static void Hierarchy()
    {
        var renderer = NewRenderer();
        var owner = renderer.gameObject;
        Ensure(!typeof(Behaviour).IsAssignableFrom(typeof(CanvasRenderer)), "a CanvasRenderer is a Component and not a Behaviour");
        Ensure(EngineObject.FindObjectsByType<CanvasRenderer>(FindObjectsSortMode.None).Contains(renderer), "FindObjectsByType finds it");
        Throws<DisunityException>(() => owner.AddComponent<CanvasRenderer>(), "a second renderer on one object", "duplicate");
        renderer.SetColor(Color.red);
        renderer.cull = true;
        renderer.materialCount = 1;
        renderer.SetMaterial(new Material(Material.SpriteUnlit), 0);
        var clone = EngineObject.Instantiate(renderer);
        Ensure(clone != renderer && clone.gameObject != owner, "a separate renderer on a separate object");
        Ensure(clone.GetColor() == Color.red && clone.cull && clone.materialCount == 1 && clone.GetMaterial() is not null, "the clone starts with the original's state");
        clone.SetColor(Color.blue);
        clone.materialCount = 0;
        Ensure(renderer.GetColor() == Color.red && renderer.materialCount == 1, "changing the clone leaves the original");
        EngineObject.Destroy(owner);
        DisunityHost.Step(0.01f);
        Ensure(!renderer && clone, "destroying the object destroys its renderer and leaves the clone");
    }

    private const string RendererPlan = """
        {"scene":"level0","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[
        {"name":"Label","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[
        {"type":"CanvasRenderer","cull_transparent_mesh":true},
        {"type":"Script","class":"CanvasRendererTests/Holder","assembly":"ASSEMBLY","enabled":true,"fields":{"renderer":{"$component":[0,0]}}}]}]}
        """;

    private static void Loaded()
    {
        var assembly = typeof(CanvasRendererTests).Assembly;
        var plan = RendererPlan.Replace("ASSEMBLY", assembly.GetName().Name);
        using var loaded = DisunityHost.LoadScene(plan, new SceneLoadOptions(assembly, _ => []) { Assemblies = _ => assembly });
        var renderer = loaded.Objects[0].GetComponent<CanvasRenderer>()!;
        Ensure(renderer.cullTransparentMesh && !renderer.cull, "the plan's cull_transparent_mesh");
        var holder = loaded.Objects[0].GetComponent<Holder>()!;
        Ensure(ReferenceEquals(holder.renderer, renderer), "the script field is bound to the renderer");
    }

    public sealed class Holder : MonoBehaviour
    {
        public CanvasRenderer? renderer = null;
    }
}
