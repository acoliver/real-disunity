using System.Text;
using Disunity.Native;

namespace UnityEngine;

/// <summary>The calls that make renderer lists in the world and record their draws. A list is made at once, from a culling result
/// the world must already hold, and the draw joins the offscreen list in order with the other work.</summary>
internal static unsafe partial class WorldGraphics
{
    /// <summary>Makes a list of the renderers a culling result kept that have a pass named in <paramref name="passNames"/>, and
    /// returns its id. <paramref name="overrideMaterial"/> is a world material id, or 0 for none.</summary>
    internal static ulong CreateDrawList(ulong cull, string[] passNames, int queueMin, int queueMax, uint layerMask, uint renderingLayerMask,
        uint sorting, uint perObject, ulong overrideMaterial, int overridePass)
    {
        var world = World;
        var tags = Encoding.UTF8.GetBytes(string.Join('\0', passNames));
        var desc = new DisunityDrawRendererList
        {
            cull = cull,
            override_material = overrideMaterial,
            queue_min = queueMin,
            queue_max = queueMax,
            layer_mask = layerMask,
            rendering_layer_mask = renderingLayerMask,
            sorting = sorting,
            per_object = perObject,
            override_pass = overridePass,
            tags_len = (uint)tags.Length,
        };
        ulong list;
        fixed (byte* pointer = tags) Check(NativeBindings.disunity_renderer_list_create_draw(world, &desc, tags.Length == 0 ? null : pointer, &list));
        return list;
    }

    /// <summary>Makes a list of the shadow casters a culling result kept for one light. <paramref name="planes"/> holds four floats
    /// per split culling plane: the normal, then the distance. With <paramref name="layerTest"/> a caster draws only when its rendering
    /// layer mask shares a bit with the light's.</summary>
    internal static ulong CreateShadowList(ulong cull, int lightIndex, float[] planes, Vector4 sphere, uint objectsFilter, uint projection,
        bool layerTest)
    {
        var world = World;
        var desc = new DisunityShadowRendererList
        {
            cull = cull,
            light_index = lightIndex,
            plane_count = (uint)(planes.Length / 4),
            sphere_x = sphere.x,
            sphere_y = sphere.y,
            sphere_z = sphere.z,
            sphere_radius = sphere.w,
            objects_filter = objectsFilter,
            projection = projection,
            rendering_layer_test = layerTest ? 1u : 0u,
        };
        ulong list;
        fixed (float* pointer = planes) Check(NativeBindings.disunity_renderer_list_create_shadow(world, &desc, planes.Length == 0 ? null : pointer, &list));
        return list;
    }

    /// <summary>Records that the list draws into the targets bound at this point of the offscreen list.</summary>
    internal static void RecordDrawList(ulong list) => Check(NativeBindings.disunity_offscreen_draw_renderer_list(World, list));
}
