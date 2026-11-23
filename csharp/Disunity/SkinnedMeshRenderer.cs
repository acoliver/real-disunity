using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A <see cref="Renderer"/> that draws a mesh deformed by bones. The native component holds the renderer settings and
/// the skin: the identity the host draws the shared mesh by and the root bone with its bind pose. The quality, the blend
/// shape weights and the bounds are stored on the managed component. The engine does not deform the shared mesh yet, so
/// the host draws it in its bind pose, where the root bone's matrix times its bind pose puts it (the matrix every bone's
/// matrix times its bind pose gives while the bones are at rest). A renderer whose root bone is not one of its bones, or
/// whose mesh has no bind pose for it, has no placement and is left out of frames. The bind pose is read when the mesh,
/// the bones or the root bone is assigned. <see cref="BakeMesh(Mesh)"/> copies the shared mesh and refuses a renderer that
/// has bones or a nonzero blend shape weight. <see cref="localBounds"/> is the shared mesh's bounds until it is set.
/// A clone keeps the mesh and settings, and its bones and root bone are the clones of the original's where those
/// are in the cloned hierarchy and the original's otherwise.
/// </summary>
public sealed class SkinnedMeshRenderer : Renderer
{
    private readonly Dictionary<int, float> _weights = new();
    private Bounds? _localBounds;
    private Mesh? _sharedMesh;
    private Transform? _rootBone;
    private Transform[] _bones = [];

    private unsafe DisunityMeshRenderer Read()
    {
        DisunityMeshRenderer value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }

    internal override DisunityMeshRenderer ReadSettings() => Read();

    internal override unsafe void WriteSettings(DisunityMeshRenderer value) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_write(DisunityHost.WorldHandle, Handle, &value));

    internal override Mesh? BoundsMesh => sharedMesh;

    public Mesh? sharedMesh
    {
        get => _sharedMesh;
        set
        {
            _sharedMesh = value;
            WriteSkin();
        }
    }

    public Transform? rootBone
    {
        get => _rootBone;
        set
        {
            _rootBone = value;
            WriteSkin();
        }
    }

    public Transform[] bones
    {
        get => [.. _bones];
        set
        {
            _bones = [.. value ?? throw new ArgumentNullException(nameof(value))];
            WriteSkin();
        }
    }

    /// <summary>Sends the world the identity the host draws the shared mesh by and the root bone with its bind pose.
    /// Asking for the mesh's identity stores a dirty mesh and can be refused, so it comes before the write.</summary>
    private unsafe void WriteSkin()
    {
        var drawn = _sharedMesh is { } mesh && mesh ? mesh.RendererIdentity : 0UL;
        var values = stackalloc float[16];
        var pose = RootBindPose();
        if (pose is { } matrix)
            for (var i = 0; i < 16; i++) values[i] = matrix[i];
        var root = pose is null ? 0UL : _rootBone!.gameObject.NativeHandle;
        DisunityHost.ThrowIfError(NativeBindings.disunity_skinned_renderer_write(DisunityHost.WorldHandle, Handle, drawn, root, pose is null ? null : values));
    }

    /// <summary>The shared mesh's bind pose of the root bone, or null when there is no root bone among the bones, the mesh has no
    /// bind pose for its slot or the pose is not finite.</summary>
    private Matrix4x4? RootBindPose()
    {
        if (_rootBone is not { } root || !root || _sharedMesh is not { } mesh || !mesh) return null;
        var slot = Array.FindIndex(_bones, bone => ReferenceEquals(bone, root));
        if (mesh.BindPoseAt(slot) is not { } pose) return null;
        for (var i = 0; i < 16; i++)
            if (!float.IsFinite(pose[i])) return null;
        return pose;
    }

    public SkinQuality quality { get; set; }

    public bool updateWhenOffscreen { get; set; }

    public override Bounds localBounds
    {
        get => _localBounds ?? base.localBounds;
        set => _localBounds = value;
    }

    /// <summary>The weight of a blend shape, 0 until it is set. The index is not checked against the mesh, which has
    /// no blend shapes.</summary>
    public float GetBlendShapeWeight(int index) => _weights.GetValueOrDefault(CheckIndex(index));

    public void SetBlendShapeWeight(int index, float value) => _weights[CheckIndex(index)] = value;

    private static int CheckIndex(int index) =>
        index >= 0 ? index : throw new ArgumentOutOfRangeException(nameof(index), index, "the blend shape index is not negative");

    /// <summary>Replaces the data of <paramref name="mesh"/> with a copy of the shared mesh, which is the baked mesh because
    /// nothing deforms it; with no shared mesh it empties <paramref name="mesh"/>. A renderer with bones or a nonzero blend shape
    /// weight throws <see cref="NotSupportedException"/> and leaves <paramref name="mesh"/> as it was: the shared mesh holds
    /// no bone weights, bind poses or blend shapes to deform by. A copy of the shared mesh is not the baked mesh of a renderer with
    /// bones, because the bones pose it and its bounds change with the pose. Old World's <c>MeshUIScaler</c> reads only the baked
    /// mesh's bounds; a scene plan carries no bones for its renderer yet, so it copies the mesh today and meets the throw only
    /// when the importer plans the bones.</summary>
    public void BakeMesh(Mesh mesh) => BakeMesh(mesh, false);

    /// <summary>As <see cref="BakeMesh(Mesh)"/> with <paramref name="useScale"/> false. True asks for the transform's scale to
    /// be applied to the vertices, normals and tangents, which the engine does not do, so it throws
    /// <see cref="NotSupportedException"/>.</summary>
    public void BakeMesh(Mesh mesh, bool useScale)
    {
        ArgumentNullException.ThrowIfNull(mesh);
        if (_bones.Length > 0)
            throw new NotSupportedException("SkinnedMeshRenderer.BakeMesh is not supported yet for a renderer with bones: the engine's mesh holds no bone weights or bind poses to skin by");
        if (_weights.Values.Any(weight => weight != 0f))
            throw new NotSupportedException("SkinnedMeshRenderer.BakeMesh is not supported yet for a renderer with a nonzero blend shape weight: the engine's mesh holds no blend shapes");
        if (useScale)
            throw new NotSupportedException("SkinnedMeshRenderer.BakeMesh is not supported yet with useScale: the engine does not apply the transform's scale to the baked vertices");
        if (sharedMesh is { } source) mesh.CopyDataFrom(source);
        else mesh.CopyDataFrom(new Mesh());
    }

    internal override void CopyManagedState(NativeComponent source, Func<Object, Object> remap)
    {
        base.CopyManagedState(source, remap);
        var original = (SkinnedMeshRenderer)source;
        quality = original.quality;
        updateWhenOffscreen = original.updateWhenOffscreen;
        _localBounds = original._localBounds;
        foreach (var (index, weight) in original._weights) _weights[index] = weight;
        _sharedMesh = original._sharedMesh;
        _rootBone = Remapped(original._rootBone, remap);
        _bones = [.. original._bones.Select(bone => Remapped(bone, remap)!)];
        WriteSkin();
    }

    private static Transform? Remapped(Transform? bone, Func<Object, Object> remap) =>
        bone is null ? null : (Transform)remap(bone);
}
