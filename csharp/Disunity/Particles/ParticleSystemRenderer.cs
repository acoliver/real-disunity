using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>
/// The <see cref="Renderer"/> of a <see cref="ParticleSystem"/>. The native component holds the common renderer
/// settings; the particle settings are stored on the managed component and read back as written, and nothing draws
/// particles yet. Its bounds are the zero box. Defaults are the engine's choice: billboards facing the view, a
/// maximum particle size of 0.5 and no minimum.
/// </summary>
public sealed class ParticleSystemRenderer : Renderer
{
    private readonly ParticleSettings _settings = new();

    private ModuleStore Store => new(_settings, "renderer");

    private unsafe DisunityMeshRenderer Read()
    {
        DisunityMeshRenderer value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }

    internal override DisunityMeshRenderer ReadSettings() => Read();

    internal override unsafe void WriteSettings(DisunityMeshRenderer value) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_write(DisunityHost.WorldHandle, Handle, &value));

    public ParticleSystemRenderMode renderMode { get => Store.Get(ParticleSystemRenderMode.Billboard); set => Store.Set(value); }
    public Mesh? mesh { get => Store.Get<Mesh?>(null); set => Store.Set(value); }
    public float minParticleSize { get => Store.Get(0f); set => Store.Set(value); }
    public float maxParticleSize { get => Store.Get(0.5f); set => Store.Set(value); }
    public float sortingFudge { get => Store.Get(0f); set => Store.Set(value); }
    public ParticleSystemRenderSpace alignment { get => Store.Get(ParticleSystemRenderSpace.View); set => Store.Set(value); }
    public ParticleSystemSortMode sortMode { get => Store.Get(ParticleSystemSortMode.None); set => Store.Set(value); }
    public Material? trailMaterial { get => Store.Get<Material?>(null); set => Store.Set(value); }
    public float velocityScale { get => Store.Get(0f); set => Store.Set(value); }
    public float lengthScale { get => Store.Get(2f); set => Store.Set(value); }
    public float cameraVelocityScale { get => Store.Get(0f); set => Store.Set(value); }
    public Vector3 pivot { get => Store.Get(Vector3.zero); set => Store.Set(value); }
    public bool allowRoll { get => Store.Get(true); set => Store.Set(value); }

    internal override void CopyManagedState(NativeComponent source, Func<Object, Object> remap)
    {
        base.CopyManagedState(source, remap);
        _settings.CopyFrom(((ParticleSystemRenderer)source)._settings);
    }
}
