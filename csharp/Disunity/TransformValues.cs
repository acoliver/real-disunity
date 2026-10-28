using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

public partial class Transform
{
    private unsafe DisunityTransform ReadLocal()
    {
        DisunityTransform value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_transform_read(DisunityHost.WorldHandle, gameObject.NativeHandle, &value));
        return value;
    }
    private unsafe void WriteLocal(DisunityTransform value)
    {
        DisunityHost.ThrowIfError(NativeBindings.disunity_transform_write(DisunityHost.WorldHandle, gameObject.NativeHandle, &value));
        MarkChanged();
    }
    public unsafe Quaternion rotation
    {
        get
        {
            float x, y, z, w;
            DisunityHost.ThrowIfError(NativeBindings.disunity_rotation_read(DisunityHost.WorldHandle, gameObject.NativeHandle, &x, &y, &z, &w));
            return new(x, y, z, w);
        }
        set
        {
            DisunityHost.ThrowIfError(NativeBindings.disunity_rotation_write(DisunityHost.WorldHandle, gameObject.NativeHandle, value.x, value.y, value.z, value.w));
            MarkChanged();
        }
    }
    public Vector3 localScale
    {
        get { var value = ReadLocal(); return new(value.sx, value.sy, value.sz); }
        set { var current = ReadLocal(); current.sx = value.x; current.sy = value.y; current.sz = value.z; WriteLocal(current); }
    }
    public Vector3 localPosition
    {
        get { var value = ReadLocal(); return new(value.x, value.y, value.z); }
        set { var current = ReadLocal(); current.x = value.x; current.y = value.y; current.z = value.z; WriteLocal(current); }
    }
    public Quaternion localRotation
    {
        get { var value = ReadLocal(); return new(value.qx, value.qy, value.qz, value.qw); }
        set { var current = ReadLocal(); current.qx = value.x; current.qy = value.y; current.qz = value.z; current.qw = value.w; WriteLocal(current); }
    }
}
