using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

public sealed class PhysicsMaterial2D : Object
{
    private static readonly Dictionary<ulong, PhysicsMaterial2D> Materials = new();
    private readonly ulong _world;
    private readonly ulong _handle;
    public string name { get; set; }
    internal override GameObject Owner => throw new NotSupportedException("physics materials have no GameObject");
    internal override bool Destroyed => _world != DisunityHost.WorldHandle;
    public PhysicsMaterial2D() : this("") { }
    public unsafe PhysicsMaterial2D(string name)
    {
        _world = DisunityHost.WorldHandle;
        ulong handle;
        DisunityHost.ThrowIfError(NativeBindings.disunity_physics_material_create(_world, &handle));
        _handle = handle;
        this.name = name;
        Materials.Add(handle, this);
    }
    private unsafe DisunityPhysicsMaterial2d Read()
    {
        DisunityPhysicsMaterial2d value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_physics_material_read(_world, _handle, &value));
        return value;
    }
    private unsafe void Write(DisunityPhysicsMaterial2d value) => DisunityHost.ThrowIfError(NativeBindings.disunity_physics_material_write(_world, _handle, &value));
    public float friction { get => Read().friction; set { var material = Read(); material.friction = value; Write(material); } }
    public float bounciness { get => Read().bounciness; set { var material = Read(); material.bounciness = value; Write(material); } }
    internal static unsafe PhysicsMaterial2D? Get(ulong component)
    {
        ulong handle;
        DisunityHost.ThrowIfError(NativeBindings.disunity_shared_material_read(DisunityHost.WorldHandle, component, &handle));
        return handle == 0 ? null : Materials[handle];
    }
    internal static void Set(ulong component, PhysicsMaterial2D? value) => DisunityHost.ThrowIfError(NativeBindings.disunity_shared_material_write(DisunityHost.WorldHandle, component, value?._handle ?? 0));
    internal static void Clear() => Materials.Clear();
}
