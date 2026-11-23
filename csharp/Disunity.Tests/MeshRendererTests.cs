using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;
using EngineObject = UnityEngine.Object;

/// <summary>MeshFilter, MeshRenderer and SkinnedMeshRenderer: mesh sharing and instancing, baking, blend weights,
/// attaching, cloning, destroying and building from a player plan.</summary>
internal static class MeshRendererTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            SharedAndInstancedMesh();
            Console.WriteLine("PASS: MeshFilter.sharedMesh is the assigned mesh; mesh makes one named copy and assigns it");
            MeshRendererMembers();
            Console.WriteLine("PASS: MeshRenderer stores additionalVertexStreams and has subMeshStartIndex 0");
            Skinned();
            Console.WriteLine("PASS: SkinnedMeshRenderer stores mesh, bones, quality and weights; BakeMesh copies the mesh when nothing deforms it and refuses bones, blend shape weights and useScale");
            Attach();
            Console.WriteLine("PASS: AddComponent builds the three components, one of each per object, and they are found by type");
            Cloning();
            Console.WriteLine("PASS: Instantiate clones settings, materials, mesh references and bones; Destroying the object removes the components");
            Loaded();
            Console.WriteLine("PASS: a player plan builds a MeshFilter, MeshRenderer and SkinnedMeshRenderer with their settings");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static GameObject NewObject() => RendererTests.NewObject();

    private static void SharedAndInstancedMesh()
    {
        var filter = NewObject().AddComponent<MeshFilter>();
        Ensure(filter.sharedMesh is null && filter.mesh is null, "no mesh at first");
        var shared = RendererTests.Cube();
        filter.sharedMesh = shared;
        Ensure(ReferenceEquals(filter.sharedMesh, shared), "the shared mesh is the one assigned");
        var instance = filter.mesh!;
        Ensure(instance != shared && instance.name == "Cube Instance", "the first mesh read makes a named copy");
        Ensure(instance.vertexCount == 3 && instance.bounds == shared.bounds, "the copy has the data");
        Ensure(ReferenceEquals(filter.mesh, instance) && ReferenceEquals(filter.sharedMesh, instance), "later reads return it and it is shared");
        instance.Clear();
        Ensure(shared.vertexCount == 3, "editing the copy leaves the original");
        filter.mesh = shared;
        Ensure(ReferenceEquals(filter.sharedMesh, shared), "setting mesh assigns the shared mesh");
        Ensure(filter.mesh != instance, "the next read copies again");
        filter.sharedMesh = null;
        Ensure(filter.sharedMesh is null && filter.mesh is null, "null clears it");
        var gone = new Mesh();
        EngineObject.DestroyImmediate(gone);
        Throws<ArgumentException>(() => filter.sharedMesh = gone, "a destroyed mesh", "destroyed");
    }

    private static void MeshRendererMembers()
    {
        var r = NewObject().AddComponent<MeshRenderer>();
        Ensure(r.additionalVertexStreams is null && r.subMeshStartIndex == 0, "defaults");
        var streams = new Mesh();
        r.additionalVertexStreams = streams;
        Ensure(ReferenceEquals(r.additionalVertexStreams, streams), "stored");
    }

    private static void Skinned()
    {
        var owner = NewObject();
        var r = owner.AddComponent<SkinnedMeshRenderer>();
        Ensure(r.sharedMesh is null && r.rootBone is null && r.bones.Length == 0, "empty at first");
        Ensure(r.quality == SkinQuality.Auto && !r.updateWhenOffscreen, "defaults");
        var bone = NewObject().transform;
        r.rootBone = bone; r.bones = [bone]; r.quality = SkinQuality.Bone2; r.updateWhenOffscreen = true;
        Ensure(r.rootBone == bone && r.bones.Length == 1 && r.bones[0] == bone, "bones");
        var spare = NewObject().transform;
        r.bones = [bone, null!, spare];
        Ensure(r.bones.Length == 3 && r.bones[1] is null, "an empty bone slot is kept");
        r.bones = [bone];
        r.rootBone = null;
        Ensure(r.rootBone is null, "no root bone");
        r.rootBone = bone;
        Ensure(r.quality == SkinQuality.Bone2 && r.updateWhenOffscreen, "quality and offscreen");
        Ensure(r.GetBlendShapeWeight(2) == 0f, "a weight starts at 0");
        r.SetBlendShapeWeight(2, 55f);
        Ensure(r.GetBlendShapeWeight(2) == 55f && r.GetBlendShapeWeight(1) == 0f, "weights are stored per index");
        Throws<ArgumentOutOfRangeException>(() => r.SetBlendShapeWeight(-1, 1f), "a negative blend shape index");
        var shared = RendererTests.Cube();
        r.sharedMesh = shared;
        Ensure(SkinSupport.Placed(r) is null, "a mesh with no bind pose for the root bone has no placement and is left out");
        PlacedByItsRootBone(r, shared, bone, spare);
        Ensure(r.localBounds == shared.bounds && r.bounds.size == shared.bounds.size, "bounds come from the mesh");
        r.localBounds = new Bounds(Vector3.zero, Vector3.one);
        Ensure(r.localBounds.size == Vector3.one, "local bounds can be set");
        var target = new Mesh();
        BakeRefusesWhatTheMeshCannotCarry(r, target);
        r.BakeMesh(target);
        Ensure(target.vertexCount == 3 && target.GetIndexCount(0) == 3 && target.bounds == shared.bounds, "BakeMesh copies the shared mesh");
        Ensure(target.vertices[1] == shared.vertices[1], "unskinned, the vertices are the same");
        var other = new Mesh();
        r.BakeMesh(other, false);
        Ensure(other.vertexCount == 3 && other.bounds == shared.bounds, "BakeMesh without useScale is the single-argument bake");
        r.SetBlendShapeWeight(2, 0f);
        r.BakeMesh(other);
        Ensure(other.vertexCount == 3, "a blend shape weight of zero deforms nothing");
        r.sharedMesh = null;
        Ensure(SkinSupport.Placed(r) is null, "without a mesh nothing is drawn");
        r.BakeMesh(target);
        Ensure(target.vertexCount == 0, "with no mesh the target is emptied");
        Throws<ArgumentNullException>(() => r.BakeMesh(null!), "a null target");
        Throws<ArgumentNullException>(() => r.bones = null!, "null bones");
    }

    /// <summary>The mesh is drawn where the root bone's matrix times its bind pose puts it: the bone's position moves it, a
    /// root bone that is not one of the bones leaves it out, and a change of the renderer's settings does not forget the
    /// skin.</summary>
    private static void PlacedByItsRootBone(SkinnedMeshRenderer r, Mesh shared, Transform bone, Transform spare)
    {
        shared.bindposes = [Matrix4x4.Translate(new Vector3(1, 2, 3))];
        bone.position = new Vector3(3, 4, 5);
        r.sharedMesh = shared;
        var placed = SkinSupport.Placed(r);
        Ensure(placed is { } drawn && drawn.Mesh == shared.RendererIdentity && SkinSupport.PlacedAtWorld(drawn.Matrix, 4, 6, 8), "the mesh is placed at the root bone's matrix times its bind pose");
        bone.position = new Vector3(-2, 0, 1);
        Ensure(SkinSupport.Placed(r) is { } moved && SkinSupport.PlacedAtWorld(moved.Matrix, -1, 2, 4), "the placement follows the root bone");
        r.enabled = false;
        Ensure(SkinSupport.Placed(r) is null, "a disabled renderer is not drawn");
        r.enabled = true;
        Ensure(SkinSupport.Placed(r) is { } kept && kept.Mesh == shared.RendererIdentity && SkinSupport.PlacedAtWorld(kept.Matrix, -1, 2, 4), "switching the renderer on again draws the same skin");
        r.bones = [spare];
        Ensure(SkinSupport.Placed(r) is null, "a root bone that is not one of the bones has no placement");
        r.bones = [bone];
        Ensure(SkinSupport.Placed(r) is not null, "naming the root bone among the bones places the mesh again");
    }

    /// <summary>Deforming inputs the engine's mesh cannot carry (bone weights, bind poses, blend shapes) and a scale to apply
    /// are refused by name instead of baked as if they were absent. The renderer leaves with no bones and a zero weight.</summary>
    private static void BakeRefusesWhatTheMeshCannotCarry(SkinnedMeshRenderer r, Mesh target)
    {
        r.SetBlendShapeWeight(2, 0f);
        var error = Expect.Catch<NotSupportedException>(() => r.BakeMesh(target), "BakeMesh with bones");
        Ensure(error.Message.Contains("SkinnedMeshRenderer.BakeMesh") && error.Message.Contains("bones") && error.Message.Contains("not supported yet"), $"the message names the member and the bones: {error.Message}");
        Ensure(target.vertexCount == 0, "a refused bake leaves the target as it was");
        r.bones = [];
        error = Expect.Catch<NotSupportedException>(() => r.BakeMesh(target, true), "BakeMesh with useScale");
        Ensure(error.Message.Contains("SkinnedMeshRenderer.BakeMesh") && error.Message.Contains("useScale"), $"the message names the member and useScale: {error.Message}");
        r.SetBlendShapeWeight(2, 55f);
        error = Expect.Catch<NotSupportedException>(() => r.BakeMesh(target), "BakeMesh with a blend shape weight");
        Ensure(error.Message.Contains("SkinnedMeshRenderer.BakeMesh") && error.Message.Contains("blend shape"), $"the message names the member and the blend shapes: {error.Message}");
        r.SetBlendShapeWeight(2, 0f);
    }

    private static void Attach()
    {
        var owner = NewObject();
        var filter = owner.AddComponent<MeshFilter>();
        var renderer = owner.AddComponent<MeshRenderer>();
        Ensure(owner.GetComponent<MeshFilter>() == filter && owner.GetComponent<MeshRenderer>() == renderer, "found by type");
        Ensure(owner.GetComponent<Renderer>() == renderer, "found as a Renderer");
        Ensure(EngineObject.FindObjectsByType<MeshRenderer>(FindObjectsSortMode.None).Contains(renderer), "FindObjectsByType finds it");
        Throws<DisunityException>(() => owner.AddComponent<MeshFilter>(), "a second filter", "duplicate");
        Throws<DisunityException>(() => owner.AddComponent<MeshRenderer>(), "a second renderer", "duplicate");
        Throws<DisunityException>(() => owner.AddComponent<SkinnedMeshRenderer>(), "a skinned renderer beside a mesh renderer", "duplicate");
    }

    private static void Cloning()
    {
        var root = NewObject();
        var boneObject = NewObject();
        boneObject.transform.SetParent(root.transform);
        var filter = root.AddComponent<MeshFilter>();
        var shared = RendererTests.Cube();
        filter.sharedMesh = shared;
        var skinned = root.AddComponent<SkinnedMeshRenderer>();
        var source = new Material(Material.SpriteUnlit) { name = "M" };
        skinned.sharedMaterials = [source];
        var copy = skinned.material!;
        skinned.shadowCastingMode = ShadowCastingMode.TwoSided;
        skinned.sortingOrder = 9;
        shared.bindposes = [Matrix4x4.identity, Matrix4x4.identity];
        skinned.sharedMesh = shared;
        skinned.rootBone = root.transform;
        skinned.bones = [root.transform, boneObject.transform];
        skinned.SetBlendShapeWeight(0, 3f);
        var block = new MaterialPropertyBlock();
        block.SetFloat("_A", 1f);
        skinned.SetPropertyBlock(block);

        var clone = EngineObject.Instantiate(root);
        var cr = clone.GetComponent<SkinnedMeshRenderer>()!;
        Ensure(cr != skinned && ReferenceEquals(clone.GetComponent<MeshFilter>()!.sharedMesh, shared), "the clone has its own components and the same mesh");
        Ensure(cr.shadowCastingMode == ShadowCastingMode.TwoSided && cr.sortingOrder == 9, "settings are cloned");
        Ensure(ReferenceEquals(cr.sharedMaterial, copy) && ReferenceEquals(cr.material, copy), "materials are shared with the source's copies");
        Ensure(ReferenceEquals(cr.sharedMesh, shared) && cr.GetBlendShapeWeight(0) == 3f, "mesh and weights are cloned");
        Ensure(cr.rootBone == clone.transform && cr.bones.Length == 2 && cr.bones[1] == clone.transform.GetChild(0), "bones point into the clone");
        root.transform.position = new Vector3(3, 0, 0);
        clone.transform.position = new Vector3(0, 6, 0);
        Ensure(SkinSupport.Placed(cr) is { } clonePlaced && clonePlaced.Mesh == shared.RendererIdentity && SkinSupport.PlacedAtWorld(clonePlaced.Matrix, 0, 6, 0), "the clone is placed by its own root bone, not the original's");
        Ensure(SkinSupport.Placed(skinned) is { } originalPlaced && SkinSupport.PlacedAtWorld(originalPlaced.Matrix, 3, 0, 0), "the original keeps its own root bone");
        var read = new MaterialPropertyBlock();
        cr.GetPropertyBlock(read);
        Ensure(read.HasFloat("_A"), "the property block is cloned");
        cr.sortingOrder = 1;
        cr.sharedMaterial = source;
        Ensure(skinned.sortingOrder == 9 && ReferenceEquals(skinned.sharedMaterial, copy), "changing the clone leaves the original");
        var meshOnly = NewObject().AddComponent<MeshRenderer>();
        meshOnly.forceRenderingOff = true;
        Ensure(EngineObject.Instantiate(meshOnly).forceRenderingOff, "a mesh renderer clones");

        EngineObject.Destroy(root);
        DisunityHost.Step(0.01f);
        Ensure(!skinned && !filter && clone, "destroying the object destroys its renderers and leaves the clone");
        EngineObject.Destroy(clone);
        DisunityHost.Step(0.01f);
        Ensure(!cr && !clone, "destroying the clone's object destroys its renderer");
    }

    private const string Plan = """
        {"scene":"level0","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[
        {"name":"Mesh","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[
        {"type":"MeshFilter"},
        {"type":"MeshRenderer","enabled":false,"receive_shadows":false,"dynamic_occludee":false,"static_shadow_caster":true,"shadow_casting_mode":3,"motion_vector_mode":2,"light_probe_usage":2,"reflection_probe_usage":3,"rendering_layer_mask":4294967295,"renderer_priority":-4,"sorting_layer_id":0,"sorting_order":7,"materials":[null,null]}]},
        {"name":"Skin","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[
        {"type":"SkinnedMeshRenderer","enabled":true,"receive_shadows":true,"dynamic_occludee":true,"static_shadow_caster":false,"shadow_casting_mode":1,"motion_vector_mode":1,"light_probe_usage":1,"reflection_probe_usage":1,"rendering_layer_mask":1,"renderer_priority":0,"sorting_layer_id":0,"sorting_order":0,"materials":[null],"quality":4,"update_when_offscreen":true,"mesh":null,"bones":[0,null],"root_bone":0}]}]}
        """;

    private static void Loaded()
    {
        var assembly = typeof(MeshRendererTests).Assembly;
        using var loaded = DisunityHost.LoadScene(Plan, new SceneLoadOptions(assembly, _ => []) { Assemblies = _ => assembly });
        var mesh = loaded.Objects[0];
        Ensure(mesh.GetComponent<MeshFilter>()!.sharedMesh is null, "the filter has no mesh");
        var r = mesh.GetComponent<MeshRenderer>()!;
        Ensure(!r.enabled && !r.receiveShadows && !r.allowOcclusionWhenDynamic && r.staticShadowCaster, "switches");
        Ensure(r.shadowCastingMode == ShadowCastingMode.ShadowsOnly && r.motionVectorGenerationMode == MotionVectorGenerationMode.ForceNoMotion, "modes");
        Ensure(r.lightProbeUsage == LightProbeUsage.UseProxyVolume && r.reflectionProbeUsage == ReflectionProbeUsage.Simple, "probes");
        Ensure(r.renderingLayerMask == uint.MaxValue && r.rendererPriority == -4 && r.sortingOrder == 7, "mask, priority and order");
        Ensure(r.sharedMaterials.Length == 2 && r.sharedMaterials.All(m => m is null), "two empty material slots");
        var s = loaded.Objects[1].GetComponent<SkinnedMeshRenderer>()!;
        Ensure(s.quality == SkinQuality.Bone4 && s.updateWhenOffscreen && s.sharedMaterials.Length == 1, "skinned settings");
        Ensure(s.sharedMesh is null && s.rootBone == mesh.transform && s.bones.Length == 2 && s.bones[0] == mesh.transform && s.bones[1] is null, "the plan's bones and root bone are the planned objects' transforms");
    }
}
