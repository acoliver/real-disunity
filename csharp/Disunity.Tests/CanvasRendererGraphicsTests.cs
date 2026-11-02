using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using static GraphicsTestSupport;
using EngineObject = UnityEngine.Object;

/// <summary>A CanvasRenderer sends its mesh's first sub-mesh and its texture's renderer identity to the native world.</summary>
internal static class CanvasRendererGraphicsTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            SetMeshSendsTheVertices();
            Console.WriteLine("PASS: CanvasRenderer.SetMesh sends positions, first uvs, colors and the triangles of sub-mesh 0");
            NullAndClearDropTheGeometry();
            Console.WriteLine("PASS: CanvasRenderer.SetMesh(null) and Clear drop the native geometry");
            TexturesGiveTheirIdentity();
            Console.WriteLine("PASS: CanvasRenderer.SetTexture and SetMaterial record the renderer identity of a Texture2D");
            APlaceholderGlyphAtlasHasNoTexelsToDraw();
            Console.WriteLine("PASS: a CanvasRenderer drawing a one-texel alpha texture (an unfilled dynamic font atlas) gets an identity with no texels");
            AMaterialThatWritesNoColourHidesTheQuads();
            Console.WriteLine("PASS: a CanvasRenderer whose material writes no colour (a hidden mask graphic) gets an identity with no texels");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static CanvasRenderer NewRenderer() => DisunityHost.CreateGameObject().AddComponent<CanvasRenderer>();

    private static unsafe (DisunityUiGraphicInfo Info, DisunityUiVertex[] Vertices) Read(CanvasRenderer renderer)
    {
        var info = new DisunityUiGraphicInfo();
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_renderer_geometry_read(DisunityHost.WorldHandle, renderer.Handle, &info, null, 0));
        var vertices = new DisunityUiVertex[info.vertex_count];
        fixed (DisunityUiVertex* pointer = vertices)
            DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_renderer_geometry_read(DisunityHost.WorldHandle, renderer.Handle, &info, pointer, info.vertex_count));
        return (info, vertices);
    }

    private static Mesh QuadMesh()
    {
        var mesh = new Mesh();
        mesh.vertices = [new Vector3(-1, -2, 0), new Vector3(-1, 2, 0), new Vector3(1, 2, 0), new Vector3(1, -2, 0)];
        mesh.uv = [new Vector2(0, 0.25f), new Vector2(0, 1), new Vector2(0.5f, 1), new Vector2(0.5f, 0.25f)];
        mesh.colors = [Color.red, Color.green, Color.blue, new Color(1, 1, 1, 0.5f)];
        mesh.triangles = [0, 1, 2, 2, 3, 0];
        return mesh;
    }

    private static void SetMeshSendsTheVertices()
    {
        var renderer = NewRenderer();
        renderer.SetMesh(QuadMesh());
        var (info, vertices) = Read(renderer);
        Ensure(info.vertex_count == 4 && info.index_count == 6, $"four vertices and six indices, got {info.vertex_count} and {info.index_count}");
        Ensure(vertices[1].x == -1 && vertices[1].y == 2 && vertices[2].u == 0.5f && vertices[2].v == 1f, "position and uv0 are sent");
        Ensure(vertices[0].r == 1 && vertices[0].g == 0 && vertices[1].g == 1 && vertices[3].a == 0.5f, "colors are sent");
        var bare = new Mesh();
        bare.vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 0)];
        bare.triangles = [0, 1, 2];
        renderer.SetMesh(bare);
        var (bareInfo, bareVertices) = Read(renderer);
        Ensure(bareInfo.index_count == 3 && bareVertices[1].r == 1 && bareVertices[1].g == 1 && bareVertices[1].b == 1 && bareVertices[1].a == 1, "a mesh without colors is white");
        Ensure(bareVertices[2].u == 0 && bareVertices[2].v == 0, "a mesh without uvs has zeros");
        var shifted = new Mesh();
        shifted.vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 0), new Vector3(2, 2, 0)];
        shifted.SetTriangles(new[] { 0, 1, 2 }, 0, true, 1);
        renderer.SetMesh(shifted);
        Ensure(Read(renderer).Info.index_count == 3, "an offset sub-mesh sends its indices with the base vertex applied");
        var lines = new Mesh();
        lines.vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0)];
        lines.SetIndices(new[] { 0, 1 }, MeshTopology.Lines, 0);
        renderer.SetMesh(lines);
        var (lineInfo, _) = Read(renderer);
        Ensure(lineInfo.vertex_count == 2 && lineInfo.index_count == 0, "a mesh that is not a triangle list sends no indices");
    }

    private static void NullAndClearDropTheGeometry()
    {
        var renderer = NewRenderer();
        renderer.SetMesh(QuadMesh());
        renderer.SetMesh(null);
        Ensure(Read(renderer).Info.vertex_count == 0, "a null mesh clears the geometry");
        renderer.SetMesh(QuadMesh());
        renderer.SetTexture(new Texture2D(2, 2));
        renderer.Clear();
        var (info, _) = Read(renderer);
        Ensure(info.vertex_count == 0 && info.texture == 0, "Clear drops the geometry and the drawing texture");
    }

    private static void TexturesGiveTheirIdentity()
    {
        var renderer = NewRenderer();
        var texture = new Texture2D(2, 2);
        renderer.SetTexture(texture);
        Ensure(Read(renderer).Info.texture == texture.RendererIdentity, "a Texture2D records its renderer identity");
        renderer.SetTexture(null);
        Ensure(Read(renderer).Info.texture == 0, "no texture draws plain color");
        var empty = new Texture2D(0, 0);
        renderer.SetTexture(empty);
        var undrawn = Read(renderer).Info.texture;
        Ensure(undrawn >= DisunityHost.FirstPlayerTextureIdentity && DisunityHost.PlayerTexture(undrawn) is null,
            "a texture of no size gets an identity with no texels, so its quads are left out");
        var material = new Material(Material.SpriteUnlit);
        var fromMaterial = new Texture2D(4, 4);
        material.mainTexture = fromMaterial;
        renderer.SetMaterial(material, null);
        Ensure(Read(renderer).Info.texture == fromMaterial.RendererIdentity, "with no texture the material's main texture is drawn");
        var given = new Texture2D(2, 4);
        renderer.SetMaterial(material, given);
        Ensure(Read(renderer).Info.texture == given.RendererIdentity, "a given texture wins over the material's");
        var stored = new RenderTexture(4, 4, 0);
        renderer.SetTexture(stored);
        var target = Read(renderer).Info.texture;
        Ensure(target >= DisunityHost.FirstPlayerTextureIdentity && target != undrawn && DisunityHost.PlayerTexture(target) is null,
            "a render texture gets its own identity with no texels");
        renderer.SetTexture(null);
        renderer.SetTexture(stored);
        Ensure(Read(renderer).Info.texture == target, "a render texture keeps its identity");
        EngineObject.DestroyImmediate(stored);
    }

    private static void APlaceholderGlyphAtlasHasNoTexelsToDraw()
    {
        var renderer = NewRenderer();
        var atlas = new Texture2D(1, 1, TextureFormat.Alpha8, false);
        renderer.SetTexture(atlas);
        var undrawn = Read(renderer).Info.texture;
        Ensure(undrawn >= DisunityHost.FirstPlayerTextureIdentity && undrawn != atlas.RendererIdentity && DisunityHost.PlayerTexture(undrawn) is null,
            "a one-texel alpha texture holds no glyph, so its quads are left out");
        var filled = new Texture2D(2, 2, TextureFormat.Alpha8, false);
        renderer.SetTexture(filled);
        Ensure(Read(renderer).Info.texture == filled.RendererIdentity, "an alpha texture with more than one texel is drawn");
        var oneColour = new Texture2D(1, 1, TextureFormat.RGBA32, false);
        renderer.SetTexture(oneColour);
        Ensure(Read(renderer).Info.texture == oneColour.RendererIdentity, "a one-texel colour texture is drawn");
        renderer.SetTexture(Texture2D.whiteTexture);
        Ensure(Read(renderer).Info.texture == Texture2D.whiteTexture.RendererIdentity, "the white texture is drawn");
    }

    private static void AMaterialThatWritesNoColourHidesTheQuads()
    {
        var renderer = NewRenderer();
        var texture = new Texture2D(2, 2);
        var hidden = new Material(Canvas.GetDefaultCanvasMaterial());
        hidden.SetInt("_ColorMask", 0);
        renderer.materialCount = 1;
        renderer.SetMaterial(hidden, 0);
        renderer.SetTexture(texture);
        var undrawn = Read(renderer).Info.texture;
        Ensure(undrawn >= DisunityHost.FirstPlayerTextureIdentity && undrawn != texture.RendererIdentity && DisunityHost.PlayerTexture(undrawn) is null,
            "a texture drawn through a material with colour mask 0 has no texels");
        renderer.SetTexture(null);
        Ensure(Read(renderer).Info.texture == undrawn, "plain colour through a material with colour mask 0 is left out too");
        var shown = new Material(Canvas.GetDefaultCanvasMaterial());
        renderer.SetMaterial(shown, 0);
        Ensure(Read(renderer).Info.texture == 0, "a material that writes colour draws plain colour again");
        renderer.SetTexture(texture);
        Ensure(Read(renderer).Info.texture == texture.RendererIdentity, "a material that writes colour draws the texture");
        renderer.SetMaterial(hidden, 0);
        Ensure(Read(renderer).Info.texture == undrawn, "assigning the hiding material to a drawn renderer hides it");
        var partial = new Material(Canvas.GetDefaultCanvasMaterial());
        partial.SetInt("_ColorMask", 14);
        renderer.SetMaterial(partial, 0);
        Ensure(Read(renderer).Info.texture == texture.RendererIdentity, "a material that writes some colour channels still draws");
    }
}
