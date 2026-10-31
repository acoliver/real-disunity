using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;
using EngineObject = UnityEngine.Object;

/// <summary>The Renderer base class: settings, matrices, bounds, material instancing, property blocks and the
/// SpriteRenderer as a Renderer.</summary>
internal static class RendererTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Defaults();
            Console.WriteLine("PASS: a MeshRenderer starts enabled, shadowed, on the Default layer and without materials; it is not a Behaviour");
            Settings();
            Console.WriteLine("PASS: every Renderer setting is stored by the native component and a bad enum value throws");
            Sorting();
            Matrices();
            Console.WriteLine("PASS: Renderer sorting layer, local-to-world and world-to-local matrices");
            Bounds();
            Console.WriteLine("PASS: Renderer.bounds is the world box of the mesh bounds and the zero box without a mesh");
            SharedMaterials();
            Console.WriteLine("PASS: Renderer shared materials, material lists and counts");
            Instancing();
            Console.WriteLine("PASS: Renderer.material and materials copy once, name the copies (Instance) and assign them as shared");
            PropertyBlocks();
            Console.WriteLine("PASS: Renderer property blocks are copied in, copied out and cleared");
            Sprite();
            Console.WriteLine("PASS: SpriteRenderer is a Renderer with one material and sprite defaults, and GetComponent<Renderer> finds every kind");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    internal static GameObject NewObject() => DisunityHost.CreateGameObject();
    private static MeshRenderer NewRenderer() => NewObject().AddComponent<MeshRenderer>();
    private static Material NewMaterial(string name) => new(Material.SpriteUnlit) { name = name };

    private static void Defaults()
    {
        var r = NewRenderer();
        Ensure(r.enabled && !r.isVisible && !r.forceRenderingOff, "enabled, not visible, not forced off");
        Ensure(r.shadowCastingMode == ShadowCastingMode.On && r.receiveShadows, "shadows");
        Ensure(r.motionVectorGenerationMode == MotionVectorGenerationMode.Object, "motion vectors");
        Ensure(r.lightProbeUsage == LightProbeUsage.BlendProbes && r.reflectionProbeUsage == ReflectionProbeUsage.BlendProbes, "probes");
        Ensure(r.renderingLayerMask == 1 && r.rendererPriority == 0, "layer mask and priority");
        Ensure(r.allowOcclusionWhenDynamic && !r.staticShadowCaster, "occlusion");
        Ensure(r.sortingLayerID == 0 && r.sortingLayerName == "Default" && r.sortingOrder == 0, "sorting");
        Ensure(r.sharedMaterial is null && r.sharedMaterials.Length == 0 && r.material is null && r.materials.Length == 0, "no materials");
        Ensure(!typeof(Behaviour).IsAssignableFrom(typeof(Renderer)) && typeof(Component).IsAssignableFrom(typeof(Renderer)), "a Component, not a Behaviour");
        Ensure(typeof(Renderer).IsAbstract, "Renderer is abstract");
    }

    private static void Settings()
    {
        var r = NewRenderer();
        r.enabled = false; r.forceRenderingOff = true; r.receiveShadows = false;
        r.shadowCastingMode = ShadowCastingMode.ShadowsOnly;
        r.motionVectorGenerationMode = MotionVectorGenerationMode.ForceNoMotion;
        r.lightProbeUsage = LightProbeUsage.CustomProvided;
        r.reflectionProbeUsage = ReflectionProbeUsage.Simple;
        r.renderingLayerMask = 0xF0F0u; r.rendererPriority = -7; r.sortingOrder = 42;
        r.allowOcclusionWhenDynamic = false; r.staticShadowCaster = true;
        Ensure(!r.enabled && r.forceRenderingOff && !r.receiveShadows, "switches");
        Ensure(r.shadowCastingMode == ShadowCastingMode.ShadowsOnly && r.motionVectorGenerationMode == MotionVectorGenerationMode.ForceNoMotion, "modes");
        Ensure(r.lightProbeUsage == LightProbeUsage.CustomProvided && r.reflectionProbeUsage == ReflectionProbeUsage.Simple, "probe modes");
        Ensure(r.renderingLayerMask == 0xF0F0u && r.rendererPriority == -7 && r.sortingOrder == 42, "numbers");
        Ensure(!r.allowOcclusionWhenDynamic && r.staticShadowCaster, "flags");
        var other = NewRenderer();
        Ensure(other.enabled && other.sortingOrder == 0, "another renderer is unchanged");
        Throws<DisunityException>(() => r.shadowCastingMode = (ShadowCastingMode)9, "a shadow mode out of range");
        Throws<DisunityException>(() => r.lightProbeUsage = (LightProbeUsage)3, "a light probe usage out of range");
        Ensure(r.shadowCastingMode == ShadowCastingMode.ShadowsOnly, "a rejected value changes nothing");
    }

    private static void Sorting()
    {
        var r = NewRenderer();
        r.sortingLayerName = "Default";
        Ensure(r.sortingLayerID == 0, "Default is layer 0");
        r.sortingLayerName = "Other";
        Ensure(r.sortingLayerID == 0, "an unregistered name keeps the layer");
        r.sortingLayerID = 5;
        Ensure(r.sortingLayerName == "", "an unregistered id has no name");
    }

    private static void Matrices()
    {
        var r = NewRenderer();
        r.transform.position = new Vector3(3, 4, 5);
        r.transform.localScale = new Vector3(2, 2, 2);
        Ensure(r.localToWorldMatrix.MultiplyPoint3x4(Vector3.one) == new Vector3(5, 6, 7), "local to world");
        Ensure(r.worldToLocalMatrix.MultiplyPoint3x4(new Vector3(5, 6, 7)) == Vector3.one, "world to local");
    }

    internal static Mesh Cube()
    {
        var mesh = new Mesh { name = "Cube" };
        mesh.vertices = [new(-1, -2, -3), new(1, 2, 3), new(1, -2, 3)];
        mesh.triangles = [0, 1, 2];
        mesh.RecalculateBounds();
        return mesh;
    }

    private static void Bounds()
    {
        var owner = NewObject();
        var r = owner.AddComponent<MeshRenderer>();
        Ensure(r.bounds == default && r.localBounds == default, "no filter, zero box");
        var filter = owner.AddComponent<MeshFilter>();
        Ensure(r.bounds == default, "a filter without a mesh, zero box");
        filter.sharedMesh = Cube();
        owner.transform.position = new Vector3(10, 0, 0);
        owner.transform.localScale = new Vector3(2, 3, 4);
        Ensure(r.localBounds.size == new Vector3(2, 4, 6), "local bounds are the mesh bounds");
        Ensure(r.bounds.center == new Vector3(10, 0, 0) && r.bounds.size == new Vector3(4, 12, 24), "world box under position and scale");
        owner.transform.rotation = Quaternion.Euler(0, 0, 90);
        Near(r.bounds.size, new Vector3(12, 4, 24), "a quarter turn swaps the box's x and y extents");
        Throws<NotSupportedException>(() => r.localBounds = default, "setting a MeshRenderer's local bounds", "localBounds");
    }

    private static void Near(Vector3 actual, Vector3 expected, string what) =>
        Ensure((actual - expected).magnitude < 1e-4f, $"{what}: expected {expected}, got {actual}");

    private static void SharedMaterials()
    {
        var r = NewRenderer();
        var a = NewMaterial("A"); var b = NewMaterial("B");
        r.sharedMaterial = a;
        Ensure(r.sharedMaterial == a && r.sharedMaterials.Length == 1, "setting the shared material makes a list of one");
        r.sharedMaterials = [a, b];
        Ensure(r.sharedMaterials[1] == b && r.sharedMaterial == a, "the shared list");
        r.sharedMaterial = b;
        Ensure(r.sharedMaterials[0] == b && r.sharedMaterials[1] == b && r.sharedMaterials.Length == 2, "sharedMaterial sets slot 0 only");
        var list = new List<Material> { a };
        r.GetSharedMaterials(list);
        Ensure(list.Count == 2 && list[0] == b, "GetSharedMaterials clears the caller's list first");
        r.SetSharedMaterials([a, a, a]);
        Ensure(r.sharedMaterials.Length == 3, "SetSharedMaterials");
        r.sharedMaterials = [];
        Ensure(r.sharedMaterial is null, "an empty list leaves no material");
        Throws<ArgumentNullException>(() => r.sharedMaterials = null!, "a null list");
        Throws<DisunityException>(() => r.sharedMaterials = new Material?[65], "more than 64 materials");
        Ensure(r.sharedMaterials.Length == 0, "a rejected list changes nothing");
        var copy = r.sharedMaterials;
        Ensure(copy != r.sharedMaterials || copy.Length == 0, "the getter returns a fresh array");
    }

    private static void Instancing()
    {
        var r = NewRenderer();
        var a = NewMaterial("A"); var b = NewMaterial("B");
        r.sharedMaterials = [a, b];
        var first = r.material!;
        Ensure(first != a && first.name == "A (Instance)", "the first access copies and names");
        Ensure(ReferenceEquals(r.material, first), "a later access returns the same copy");
        Ensure(ReferenceEquals(r.sharedMaterial, first), "the copy is the shared material");
        var all = r.materials;
        Ensure(all.Length == 2 && ReferenceEquals(all[0], first) && all[1]!.name == "B (Instance)" && all[1] != b, "materials are copies, the first being the same copy");
        Ensure(ReferenceEquals(r.materials[1], all[1]), "copies are made once");
        Ensure(a.name == "A" && b.name == "B", "the sources keep their names");
        var list = new List<Material>();
        r.GetMaterials(list);
        Ensure(list.Count == 2 && ReferenceEquals(list[1], all[1]), "GetMaterials");
        r.sharedMaterial = a;
        Ensure(r.material != first && r.material!.name == "A (Instance)", "setting a shared material drops the copies");
        var mine = NewMaterial("Mine");
        r.material = mine;
        Ensure(ReferenceEquals(r.material, mine) && ReferenceEquals(r.sharedMaterial, mine), "setting material assigns it without a copy");
        r.materials = [mine, mine];
        Ensure(r.sharedMaterials.Length == 2, "setting materials");
        r.SetMaterials([mine]);
        Ensure(r.materials.Length == 1, "SetMaterials");
        var empty = NewRenderer();
        Ensure(empty.material is null && empty.materials.Length == 0, "no copies without a source");
        empty.sharedMaterial = a;
        Ensure(empty.material!.name == "A (Instance)", "a source set later is copied");
    }

    private static void PropertyBlocks()
    {
        var r = NewRenderer();
        Ensure(!r.HasPropertyBlock(), "none at first");
        var block = new MaterialPropertyBlock();
        block.SetFloat("_Cut", 0.5f);
        r.SetPropertyBlock(block);
        Ensure(r.HasPropertyBlock(), "set");
        block.SetFloat("_Cut", 0.9f);
        var read = new MaterialPropertyBlock();
        read.SetFloat("_Stale", 1f);
        r.GetPropertyBlock(read);
        Ensure(read.GetFloat("_Cut") == 0.5f && !read.HasFloat("_Stale"), "the stored block is a copy, and the caller's block is cleared first");
        var indexed = new MaterialPropertyBlock();
        indexed.SetFloat("_Only", 2f);
        r.SetPropertyBlock(indexed, 1);
        var one = new MaterialPropertyBlock();
        r.GetPropertyBlock(one, 1);
        Ensure(one.HasFloat("_Only") && !one.HasFloat("_Cut"), "per-material blocks are separate");
        r.GetPropertyBlock(one, 0);
        Ensure(one.isEmpty, "a material without a block reads empty");
        Throws<ArgumentOutOfRangeException>(() => r.SetPropertyBlock(block, -1), "a negative material index");
        r.SetPropertyBlock(null);
        Ensure(!r.HasPropertyBlock(), "null removes it");
        r.GetPropertyBlock(read);
        Ensure(read.isEmpty, "reading with none leaves the block empty");
    }

    private static void Sprite()
    {
        var owner = NewObject();
        var s = owner.AddComponent<SpriteRenderer>();
        Ensure(s is Renderer, "a SpriteRenderer is a Renderer");
        Ensure(ReferenceEquals(owner.GetComponent<Renderer>(), s), "GetComponent<Renderer> finds a sprite renderer");
        Ensure(s.enabled && s.shadowCastingMode == ShadowCastingMode.Off && !s.receiveShadows, "sprite defaults: no shadows");
        s.enabled = false; s.sortingOrder = 4; s.sortingLayerID = 3;
        Ensure(!s.enabled && s.sortingOrder == 4 && s.sortingLayerID == 3 && s.sortingLayer == 3, "enabled and sorting live in the sprite");
        s.shadowCastingMode = ShadowCastingMode.On;
        Ensure(s.shadowCastingMode == ShadowCastingMode.On && !s.enabled, "another setting keeps the sprite's own");
        Ensure(ReferenceEquals(s.sharedMaterial, Material.SpriteUnlit) && s.sharedMaterials.Length == 1, "one material, the unlit sprite material");
        s.sharedMaterial = Material.SpriteLit;
        Ensure(ReferenceEquals(s.sharedMaterial, Material.SpriteLit), "the lit sprite material");
        var copy = s.material!;
        Ensure(copy != Material.SpriteLit && copy.name == "Sprite-Lit-Default (Instance)" && copy.SpriteMaterial == BuiltinSpriteMaterial.Lit, "the copy keeps the sprite material identity");
        Ensure(ReferenceEquals(s.material, copy), "the copy is kept");
        Throws<NotSupportedException>(() => s.sharedMaterials = [], "no material on a sprite", "one material");
        Throws<NotSupportedException>(() => s.sharedMaterials = [Material.SpriteLit, Material.SpriteLit], "two materials on a sprite", "one material");
        Throws<ArgumentNullException>(() => s.sharedMaterial = null, "a null sprite material");
        Ensure(s.bounds.size == Vector3.zero, "a sprite renderer without a sprite has an empty box");
        Ensure(s.localBounds == default, "sprite local bounds are the zero box");
        var clone = EngineObject.Instantiate(s);
        Ensure(clone.shadowCastingMode == ShadowCastingMode.On && clone.sortingOrder == 4 && !clone.enabled, "a cloned sprite renderer keeps the settings");
        Ensure(ReferenceEquals(clone.material, copy), "the clone shares the source's copy");
        Ensure(ReferenceEquals(NewObject().AddComponent<SkinnedMeshRenderer>().gameObject.GetComponent<Renderer>()?.GetType(), typeof(SkinnedMeshRenderer)), "GetComponent<Renderer> finds a skinned renderer");
        Ensure(NewObject().AddComponent<MeshRenderer>().GetComponent<Renderer>() is MeshRenderer, "and a mesh renderer");
    }
}
