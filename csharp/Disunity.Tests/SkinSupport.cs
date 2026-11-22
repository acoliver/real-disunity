using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;

/// <summary>What the world holds of a skinned renderer's skin, read through the native interface.</summary>
internal static class SkinSupport
{
    internal static unsafe (ulong Mesh, ulong Root, ulong[] Bones) Read(SkinnedMeshRenderer renderer)
    {
        var skin = new DisunityRendererSkin();
        DisunityHost.ThrowIfError(NativeBindings.disunity_skinned_renderer_read(DisunityHost.WorldHandle, renderer.Handle, &skin, null, 0));
        var bones = new ulong[skin.bone_count];
        fixed (ulong* pointer = bones)
            DisunityHost.ThrowIfError(NativeBindings.disunity_skinned_renderer_read(DisunityHost.WorldHandle, renderer.Handle, &skin, pointer, skin.bone_count));
        return (skin.drawn_mesh, skin.root_bone, bones);
    }
}
