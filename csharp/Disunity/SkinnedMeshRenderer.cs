using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A <see cref="Renderer"/> that draws a mesh deformed by bones. The native component holds the renderer settings;
/// the mesh, the bones, the root bone, the quality, the blend shape weights and the bounds are stored on the managed
/// component. The engine does not skin: <see cref="BakeMesh(Mesh)"/> copies the shared mesh unchanged, and
/// the bones and weights have no effect on it. <see cref="localBounds"/> is the shared mesh's bounds until it is set.
/// A clone keeps the mesh and settings, and its bones and root bone are the clones of the original's where those
/// are in the cloned hierarchy and the original's otherwise.
/// </summary>
public sealed class SkinnedMeshRenderer : Renderer
{
    private readonly Dictionary<int, float> _weights = new();
    private Bounds? _localBounds;
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

    public Mesh? sharedMesh { get; set; }

    public Transform? rootBone { get; set; }

    public Transform[] bones
    {
        get => [.. _bones];
        set => _bones = [.. value ?? throw new ArgumentNullException(nameof(value))];
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

    /// <summary>Replaces the data of <paramref name="mesh"/> with a copy of the shared mesh, unskinned; with no shared
    /// mesh it empties <paramref name="mesh"/>.</summary>
    public void BakeMesh(Mesh mesh)
    {
        ArgumentNullException.ThrowIfNull(mesh);
        if (sharedMesh is { } source) mesh.CopyDataFrom(source);
        else mesh.CopyDataFrom(new Mesh());
    }

    /// <summary>As <see cref="BakeMesh(Mesh)"/>; <paramref name="useScale"/> has no effect because nothing is skinned.</summary>
    public void BakeMesh(Mesh mesh, bool useScale) => BakeMesh(mesh);

    internal override void CopyManagedState(NativeComponent source, Func<Object, Object> remap)
    {
        base.CopyManagedState(source, remap);
        var original = (SkinnedMeshRenderer)source;
        sharedMesh = original.sharedMesh;
        quality = original.quality;
        updateWhenOffscreen = original.updateWhenOffscreen;
        _localBounds = original._localBounds;
        foreach (var (index, weight) in original._weights) _weights[index] = weight;
        rootBone = Remapped(original.rootBone, remap);
        _bones = [.. original._bones.Select(bone => Remapped(bone, remap)!)];
    }

    private static Transform? Remapped(Transform? bone, Func<Object, Object> remap) =>
        bone is null ? null : (Transform)remap(bone);
}
