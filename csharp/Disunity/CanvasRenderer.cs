using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>
/// A native component that holds what a UI element submits to be drawn: a color, an alpha, a mesh, a main and
/// an alpha texture, materials, pop materials and a clip rectangle. Every setting is read from and written to
/// the component in the native world, so a clone from <c>Instantiate</c> has its own. A mesh, texture or
/// material is stored by instance id and found again while the object is alive, so a getter returns the object
/// that was set and returns null once it is destroyed or collected. Nothing draws these meshes yet. The engine
/// does not batch UI, so <see cref="absoluteDepth"/> and <see cref="relativeDepth"/> are always -1.
/// The material count is 0 through <see cref="MaxMaterials"/>; a larger count throws. The native side rejects a
/// NaN alpha with a <see cref="DisunityException"/> and keeps any other alpha as given. The engine does not
/// raise <see cref="onRequestRebuild"/>. A <c>RequireComponent</c> attribute on a script is not honoured by
/// <c>AddComponent</c> yet, so a graphic needs its renderer added by hand.
/// </summary>
public sealed partial class CanvasRenderer : NativeComponent
{
    /// <summary>How many materials, and how many pop materials, one renderer holds.</summary>
    public const int MaxMaterials = 8;

    public delegate void OnRequestRebuild();

    private static OnRequestRebuild? s_requestRebuild;

    /// <summary>Subscribers are kept and never called: the engine has no canvas that asks for a rebuild.</summary>
    public static event OnRequestRebuild onRequestRebuild
    {
        add => s_requestRebuild = (OnRequestRebuild?)Delegate.Combine(s_requestRebuild, value);
        remove => s_requestRebuild = (OnRequestRebuild?)Delegate.Remove(s_requestRebuild, value);
    }

    private unsafe DisunityCanvasRenderer Read()
    {
        DisunityCanvasRenderer value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_renderer_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    private unsafe void Write(DisunityCanvasRenderer value) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_renderer_write(DisunityHost.WorldHandle, Handle, &value));
    private void Update(Func<DisunityCanvasRenderer, DisunityCanvasRenderer> change) => Write(change(Read()));

    public void SetColor(Color color) => Update(r => { r.r = color.r; r.g = color.g; r.b = color.b; r.a = color.a; return r; });
    public Color GetColor() { var r = Read(); return new Color(r.r, r.g, r.b, r.a); }

    /// <summary>Kept apart from the color's alpha. A NaN alpha throws.</summary>
    public void SetAlpha(float alpha) => Update(r => { r.alpha = alpha; return r; });
    public float GetAlpha() => Read().alpha;

    /// <summary>The alpha this element is drawn with: its own alpha times the alpha of each enabled
    /// <see cref="CanvasGroup"/> on its object and above, stopping after a group that ignores its parents.</summary>
    public unsafe float GetInheritedAlpha()
    {
        float alpha;
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_renderer_inherited_alpha(DisunityHost.WorldHandle, Handle, &alpha));
        return alpha;
    }

    /// <summary>Whether this renderer's geometry is skipped.</summary>
    public bool cull { get => Read().cull == 1; set => Update(r => { r.cull = value ? 1u : 0u; return r; }); }
    public bool cullTransparentMesh { get => Read().cull_transparent_mesh == 1; set => Update(r => { r.cull_transparent_mesh = value ? 1u : 0u; return r; }); }
    public bool hasPopInstruction { get => Read().has_pop_instruction == 1; set => Update(r => { r.has_pop_instruction = value ? 1u : 0u; return r; }); }

    /// <summary>Always false: the engine does not track movement of UI elements.</summary>
    public bool hasMoved => false;

    /// <summary>Always -1: the engine does not batch UI, so no element has a depth.</summary>
    public int absoluteDepth => -1;

    /// <summary>Always -1, for the same reason as <see cref="absoluteDepth"/>.</summary>
    public int relativeDepth => -1;

    public bool hasRectClipping => Read().has_rect_clipping == 1;
    public void EnableRectClipping(Rect rect) => Update(r =>
    {
        r.has_rect_clipping = 1; r.clip_x = rect.x; r.clip_y = rect.y; r.clip_width = rect.width; r.clip_height = rect.height;
        return r;
    });
    public void DisableRectClipping() => Update(r =>
    {
        r.has_rect_clipping = 0; r.clip_x = 0f; r.clip_y = 0f; r.clip_width = 0f; r.clip_height = 0f;
        return r;
    });
    public Vector2 clippingSoftness
    {
        get { var r = Read(); return new Vector2(r.softness_x, r.softness_y); }
        set => Update(r => { r.softness_x = value.x; r.softness_y = value.y; return r; });
    }

    /// <summary>Clears the mesh, the materials (the count becomes 0) and both textures.</summary>
    public void Clear() => Update(r =>
    {
        r.mesh = 0; r.texture = 0; r.alpha_texture = 0; r.material_count = 0;
        return r;
    });

    /// <summary>Sets the mesh; null clears it. The mesh is found again by <see cref="GetMesh"/> while it is alive.</summary>
    public void SetMesh(Mesh? mesh)
    {
        var id = IdOf(mesh, nameof(mesh));
        Update(r => { r.mesh = id; return r; });
    }
    public Mesh? GetMesh() => Find<Mesh>(Read().mesh);

    public void SetTexture(Texture? texture)
    {
        var id = IdOf(texture, nameof(texture));
        Update(r => { r.texture = id; return r; });
    }
    public void SetAlphaTexture(Texture? texture)
    {
        var id = IdOf(texture, nameof(texture));
        Update(r => { r.alpha_texture = id; return r; });
    }
    internal Texture? MainTexture => Find<Texture>(Read().texture);
    internal Texture? AlphaTexture => Find<Texture>(Read().alpha_texture);

    internal static ulong IdOf(Object? value, string name)
    {
        if (value is null) return 0;
        if (!value) throw new ArgumentException($"the {name} was destroyed", name);
        return unchecked((ulong)(uint)value.GetInstanceID());
    }

    internal static T? Find<T>(ulong id) where T : Object =>
        id == 0 ? null : ObjectRegistry.Find(unchecked((int)(uint)id)) as T;
}
