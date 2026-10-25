using System.Runtime.InteropServices;
using UnityEngine;

internal static class HandshakeRunner
{
    public static void Run()
    {
        var path = TestArtifacts.Native;
        var image = NativeLibrary.Load(path);
        var create = Marshal.GetDelegateForFunctionPointer<WorldCreate>(NativeLibrary.GetExport(image, "disunity_world_create"));
        var destroy = Marshal.GetDelegateForFunctionPointer<WorldDestroy>(NativeLibrary.GetExport(image, "disunity_world_destroy"));
        BinderBehavior.Check(create(out var world).code == 0, "test world create");
        try
        {
            NativeWorld.AttachNativeLibrary(path, world);
            HandshakeBehavior.RejectBeforeAwake(path, world);
        }
        finally
        {
            NativeWorld.Detach();
            BinderBehavior.Check(destroy(world).code == 0, "test world destroy");
            NativeLibrary.Free(image);
        }
    }
}
