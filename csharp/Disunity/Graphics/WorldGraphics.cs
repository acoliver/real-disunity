using System.Text;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The calls into the native world that hold the graphics state a host draws from: shaders, materials, shader
/// properties and keywords, render textures and the offscreen work a frame records. The managed classes keep their
/// own state for reads and send every change here; the world is what draws. A managed object has a world resource
/// only after something needs one (a renderer slot, a blit, a camera target), is sent whole then, and follows every
/// later change. A resource belongs to one world, so one made for a world that was unloaded is made again.
/// </summary>
internal static unsafe partial class WorldGraphics
{
    internal const uint ScopeMaterial = 0, ScopeGlobal = 1, ScopeBlock = 2, ScopeDraw = 3, ScopeSlotBlock = 4;
    private const uint TextureNone = 0, TextureAsset = 1, TextureRender = 2, TextureUnsent = 5;
    private const uint WorkCameraRender = 0, WorkBlit = 1, WorkCopyTexture = 2;

    /// <summary>A native resource id together with the world it was made in.</summary>
    internal readonly record struct Link(ulong World, ulong Id)
    {
        /// <summary>Whether the resource belongs to the world that is loaded now.</summary>
        internal bool IsCurrent => Id != 0 && World == DisunityHost.WorldHandle && DisunityHost.IsLoaded;
    }

    /// <summary>The loaded world's handle; a resource cannot be made without one.</summary>
    internal static ulong World
    {
        get
        {
            DisunityHost.EnsureLoaded();
            return DisunityHost.IsLoaded ? DisunityHost.WorldHandle : throw new InvalidOperationException("no world is loaded");
        }
    }

    /// <summary>The world holds finite values only, so a NaN or an infinity, which a game may set, is sent as 0.</summary>
    private static float Finite(float value) => float.IsFinite(value) ? value : 0f;

    /// <summary>The most bytes a property or keyword name holds in the world.</summary>
    private const int MaxNameBytes = 256;

    /// <summary>Throws <see cref="ArgumentException"/> for a property or keyword name the world would refuse, before it is stored or
    /// recorded. A name stored and then refused would stay in the managed store, and every later world load would send it again and
    /// fail.</summary>
    internal static void CheckName(string name, string member)
    {
        ArgumentNullException.ThrowIfNull(name);
        var bytes = Encoding.UTF8.GetByteCount(name);
        if (bytes is 0 or > MaxNameBytes)
            throw new ArgumentException($"{member}: a name has 1 to {MaxNameBytes} UTF-8 bytes, not {bytes}", nameof(name));
    }

    /// <summary>The target of the global scope, which has no id of its own.</summary>
    internal static Link GlobalTarget => new(DisunityHost.WorldHandle, 1);

    private static void Check(DisunityResult result) => DisunityHost.ThrowIfError(result);

    internal static Link CreateShader(string name)
    {
        var world = World;
        var bytes = Encoding.UTF8.GetBytes(name);
        ulong id;
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_shader_create(world, pointer, (uint)bytes.Length, &id));
        return new Link(world, id);
    }

    /// <summary>Sets where the shader's programs are, or removes the source when <paramref name="file"/> is null.</summary>
    internal static void WriteShaderSource(Link shader, string? file, long pathId)
    {
        var bytes = file is null ? [] : Encoding.UTF8.GetBytes(file);
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_shader_source_write(shader.World, shader.Id, bytes.Length == 0 ? null : pointer, (uint)bytes.Length, pathId));
    }

    internal static Link CreateMaterial(Link shader)
    {
        ulong id;
        Check(NativeBindings.disunity_material_create(shader.World, shader.Id, &id));
        return new Link(shader.World, id);
    }

    internal static void DestroyMaterial(Link material) => Check(NativeBindings.disunity_material_destroy(material.World, material.Id));

    internal static void WriteMaterialShader(Link material, Link shader) => Check(NativeBindings.disunity_material_shader_write(material.World, material.Id, shader.Id));

    internal static void WriteRenderQueue(Link material, int queue) => Check(NativeBindings.disunity_material_render_queue_write(material.World, material.Id, queue));

    internal static void WriteActivePass(Link material, uint pass) => Check(NativeBindings.disunity_material_set_pass(material.World, material.Id, pass));

    internal static void WritePass(Link material, string pass, bool enabled)
    {
        var bytes = Encoding.UTF8.GetBytes(pass);
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_material_pass_write(material.World, material.Id, pointer, (uint)bytes.Length, enabled ? 1u : 0u));
    }

    internal static void WriteKeyword(uint scope, Link target, string keyword, bool enabled)
    {
        var bytes = Encoding.UTF8.GetBytes(keyword);
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_shader_keyword_write(target.World, scope, target.Id, pointer, (uint)bytes.Length, enabled ? 1u : 0u));
    }

    /// <summary>Binds the world's compute buffer <paramref name="buffer"/> to a property name of a material as a structured buffer; 0
    /// removes the binding.</summary>
    internal static void WriteMaterialBuffer(Link material, string name, ulong buffer)
    {
        var bytes = Encoding.UTF8.GetBytes(name);
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_material_set_buffer(material.World, material.Id, pointer, (uint)bytes.Length, buffer));
    }

    internal static void ClearProperties(uint scope, Link target) => Check(NativeBindings.disunity_shader_properties_clear(target.World, scope, target.Id));

    /// <summary>Sends the current value of property <paramref name="id"/> of <paramref name="store"/>.</summary>
    internal static void SendProperty(uint scope, Link target, PropertyStore store, int id, PropertyKind kind)
    {
        if (Shader.PropertyName(id) is not { } name) return;
        var bytes = Encoding.UTF8.GetBytes(name);
        fixed (byte* pointer = bytes)
        {
            var length = (uint)bytes.Length;
            switch (kind)
            {
                case PropertyKind.Float:
                    Check(NativeBindings.disunity_shader_float_write(target.World, scope, target.Id, pointer, length, Finite(store.Floats[id])));
                    break;
                case PropertyKind.Int:
                    Check(NativeBindings.disunity_shader_int_write(target.World, scope, target.Id, pointer, length, store.Ints[id]));
                    break;
                case PropertyKind.Vector:
                    var v = store.Vectors[id];
                    var vector = stackalloc float[] { Finite(v.x), Finite(v.y), Finite(v.z), Finite(v.w) };
                    Check(NativeBindings.disunity_shader_vector_write(target.World, scope, target.Id, pointer, length, vector));
                    break;
                case PropertyKind.Matrix:
                    var m = store.Matrices[id];
                    var matrix = stackalloc float[]
                    {
                        Finite(m.m00), Finite(m.m10), Finite(m.m20), Finite(m.m30), Finite(m.m01), Finite(m.m11), Finite(m.m21), Finite(m.m31),
                        Finite(m.m02), Finite(m.m12), Finite(m.m22), Finite(m.m32), Finite(m.m03), Finite(m.m13), Finite(m.m23), Finite(m.m33),
                    };
                    Check(NativeBindings.disunity_shader_matrix_write(target.World, scope, target.Id, pointer, length, matrix));
                    break;
                default:
                    var slot = SlotOf(store, id);
                    Check(NativeBindings.disunity_shader_texture_write(target.World, scope, target.Id, pointer, length, &slot));
                    break;
            }
        }
    }

    /// <summary>The texture property, scale and offset of property <paramref name="id"/>, where a missing part is no texture, scale
    /// one and offset zero.</summary>
    private static DisunityTextureSlot SlotOf(PropertyStore store, int id)
    {
        var scale = store.Scales.GetValueOrDefault(id, Vector2.one);
        var offset = store.Offsets.GetValueOrDefault(id, Vector2.zero);
        var (kind, texture) = store.Textures.TryGetValue(id, out var binding) ? TextureOf(binding.Texture, Shader.PropertyName(id) ?? "") : (TextureNone, 0UL);
        return new DisunityTextureSlot { kind = kind, texture = texture, scale_x = Finite(scale.x), scale_y = Finite(scale.y), offset_x = Finite(offset.x), offset_y = Finite(offset.y) };
    }

    /// <summary>Sends every value of <paramref name="store"/> to a material, the globals or a property block. Only the globals
    /// take the vector and matrix arrays.</summary>
    internal static void SendAll(uint scope, Link target, PropertyStore store)
    {
        foreach (var id in store.Floats.Keys) SendProperty(scope, target, store, id, PropertyKind.Float);
        foreach (var id in store.Ints.Keys) SendProperty(scope, target, store, id, PropertyKind.Int);
        foreach (var id in store.Vectors.Keys) SendProperty(scope, target, store, id, PropertyKind.Vector);
        foreach (var id in store.Matrices.Keys) SendProperty(scope, target, store, id, PropertyKind.Matrix);
        foreach (var id in store.Textures.Keys.Union(store.Offsets.Keys).Union(store.Scales.Keys).ToArray())
            SendProperty(scope, target, store, id, PropertyKind.Texture);
        if (scope == ScopeGlobal) SendGlobalArrays(store);
    }

    /// <summary>The most bytes the host stores for one texture, which its store call refuses to exceed.</summary>
    private const long MaxStoredTextureBytes = 1L << 29;

    /// <summary>Whether a texture kind reads as no texture in the world: none, and a texture the data names that was not sent.</summary>
    internal static bool IsUnbound(uint kind) => kind is TextureNone or TextureUnsent;

    /// <summary>The kind and identity a texture is sent as: a <see cref="Texture2D"/> with pixels, a
    /// <see cref="Texture2DArray"/>, a <see cref="Cubemap"/> or a <see cref="Texture3D"/> by the identity the host draws it by, a
    /// <see cref="RenderTexture"/> by its world id, null as no texture, and any other texture (a destroyed one, an empty
    /// <see cref="Texture2D"/>, an array, cube or volume the host cannot store, a kind the world does not hold) as named and not
    /// sent, which the world reads as no texture and the host's stand-in census reports as not loaded.</summary>
    internal static (uint Kind, ulong Texture) TextureOf(Texture? texture) => texture switch
    {
        null => (TextureNone, 0UL),
        Texture2D { width: > 0, height: > 0 } plain when plain => (TextureAsset, plain.RendererIdentity),
        Texture2DArray array when array => StoredOf(array, array.ByteLength, array.depth, () => array.RendererIdentity),
        Cubemap cube when cube => StoredOf(cube, cube.ByteLength, 6, () => cube.RendererIdentity),
        Texture3D volume when volume => StoredOf(volume, volume.ByteLength, volume.mipmapCount == 1 ? volume.depth : 1, () => volume.RendererIdentity),
        RenderTexture target when target => (TextureRender, target.WorldId),
        _ => (TextureUnsent, 0UL),
    };

    /// <summary>A texture of layers (the layers of an array, the faces of a cube or the slices of a volume) the host can store by its
    /// identity, or a texture named and not sent for one it cannot: no bytes, bytes that are not whole <paramref name="layers"/> (a plan's raw size
    /// may not be; a volume with a mip chain is not made of equal slices, so it passes one layer) or more than the host stores,
    /// which would otherwise make the property write throw. The first such texture is named in a warning.</summary>
    private static (uint Kind, ulong Texture) StoredOf(Texture texture, int bytes, int layers, Func<ulong> identity)
    {
        var problem = bytes == 0 ? "holds no bytes"
            : bytes > MaxStoredTextureBytes ? $"holds {bytes} bytes, more than the host stores"
            : bytes % layers != 0 ? $"holds {bytes} bytes, which are not {layers} equal layers" : null;
        if (problem is null) return (TextureAsset, identity());
        UndrawnCalls.NoteTextureUnstored(texture.name, problem);
        return (TextureUnsent, 0UL);
    }

    /// <summary>The same as <see cref="TextureOf(Texture?)"/>, and a texture that is sent as named and not sent is noted under
    /// <paramref name="property"/> with the reason (see <see cref="UnsentTextures"/>).</summary>
    internal static (uint Kind, ulong Texture) TextureOf(Texture? texture, string property)
    {
        var sent = TextureOf(texture);
        if (sent.Kind == TextureUnsent) UnsentTextures.Note(property, texture);
        return sent;
    }

    /// <summary>The same as <see cref="TextureOf(Texture?)"/> for a call that cannot draw with another kind of texture: a texture the
    /// world cannot hold throws, naming <paramref name="member"/>.</summary>
    internal static (uint Kind, ulong Texture) RequireTexture(Texture? texture, string member, string role)
    {
        if (texture is null) return (TextureNone, 0UL);
        if (!texture) throw new ArgumentException($"the {role} texture was destroyed", role);
        var (kind, identity) = TextureOf(texture);
        return kind == TextureUnsent ? throw new NotSupportedException($"{member}: a {texture.GetType().Name} {role} is not supported yet") : (kind, identity);
    }

    internal static Link CreateRenderTexture(DisunityRenderTexture desc)
    {
        var world = World;
        ulong id;
        Check(NativeBindings.disunity_render_texture_create(world, &desc, &id));
        return new Link(world, id);
    }

    internal static void WriteRenderTexture(Link texture, DisunityRenderTexture desc) =>
        Check(NativeBindings.disunity_render_texture_write(texture.World, texture.Id, &desc));

    internal static void DestroyRenderTexture(Link texture) => Check(NativeBindings.disunity_render_texture_destroy(texture.World, texture.Id));

    /// <summary>The render textures a camera draws into when its buffers were set: the world ids of the color textures in output
    /// order and of the depth texture (0 for none).</summary>
    internal readonly record struct CameraBuffers(ulong[] Colors, ulong Depth);

    /// <summary>Sets the camera's target buffers, which replace its target texture, or removes them when there are no colors.</summary>
    internal static void WriteCameraBuffers(ulong camera, ulong[] colors, ulong depth)
    {
        fixed (ulong* list = colors)
            Check(NativeBindings.disunity_camera_target_buffers_write(DisunityHost.WorldHandle, camera, colors.Length == 0 ? null : list, (uint)colors.Length, depth));
    }

    /// <summary>The camera's target buffers, or null when it has none.</summary>
    internal static CameraBuffers? ReadCameraBuffers(ulong camera)
    {
        var colors = stackalloc ulong[8];
        uint count;
        ulong depth = 0;
        Check(NativeBindings.disunity_camera_target_buffers_read(DisunityHost.WorldHandle, camera, colors, &count, &depth));
        return count == 0 ? null : new CameraBuffers(new Span<ulong>(colors, (int)count).ToArray(), depth);
    }

    /// <summary>Records a camera's render into a render texture.</summary>
    internal static void RecordCameraRender(ulong camera, ulong target)
    {
        var work = new DisunityOffscreenWork { kind = WorkCameraRender, camera = camera, target = target };
        Check(NativeBindings.disunity_offscreen_record(World, &work));
    }

    /// <summary>The target kinds a recorded blit or copy writes: a render texture by its world id, or a temporary render
    /// texture by its property name id.</summary>
    internal const uint TargetRender = 0, TargetTemporary = 1;

    /// <summary>The texture kind of a temporary render texture named by its property name id, which only recorded blit and copy
    /// sources and globals set by recorded work take.</summary>
    internal const uint TextureTemporary = 4;

    internal static void RecordBlit((uint Kind, ulong Texture) source, (uint Kind, ulong Target) target, ulong material, int pass)
    {
        var work = new DisunityOffscreenWork
        {
            kind = WorkBlit, source_kind = source.Kind, source = source.Texture, target_kind = target.Kind, target = target.Target,
            material = material, pass = pass,
        };
        Check(NativeBindings.disunity_offscreen_record(World, &work));
    }

    internal static void RecordCopy((uint Kind, ulong Texture) source, uint sourceElement, uint sourceMip, (uint Kind, ulong Target) target,
        uint targetElement, uint targetMip)
    {
        var work = new DisunityOffscreenWork
        {
            kind = WorkCopyTexture, source_kind = source.Kind, source = source.Texture, source_element = sourceElement, source_mip = sourceMip,
            target_kind = target.Kind, target = target.Target, target_element = targetElement, target_mip = targetMip,
        };
        Check(NativeBindings.disunity_offscreen_record(World, &work));
    }

    /// <summary>Writes the world material ids of a renderer's slots; a slot without a material is 0.</summary>
    internal static void WriteRendererMaterials(ulong renderer, ulong[] materials)
    {
        fixed (ulong* pointer = materials)
            Check(NativeBindings.disunity_mesh_renderer_materials_write(World, renderer, materials.Length == 0 ? null : pointer, (uint)materials.Length));
    }
}
