using System.Text.Json;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>A color a game sets as a color is stored and sent to the world in the project's color space: converted to linear
/// (RGB only) when <see cref="QualitySettings.activeColorSpace"/> is Linear, and read back as the color the game set. A vector
/// is never converted, and gamma space changes nothing. The rule covers materials, property blocks, shader globals, command
/// buffers and the colors a plan stores with a material.</summary>
internal static unsafe class ColorSpacePropertyTests
{
    private const uint MaterialScope = 0, GlobalScope = 1, BlockScope = 2;
    private static readonly Color Authored = new(0.533f, 0.051f, 0.337f, 0.6f);

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            InLinear(MaterialSetColorStoresLinear);
            Console.WriteLine("PASS: Material.SetColor in linear space stores and sends the linear color with its alpha, and GetColor returns what was set");
            InLinear(MaterialColorShortcutAndCopies);
            Console.WriteLine("PASS: Material.color, a copied material and CopyPropertiesFromMaterial convert once");
            InLinear(MaterialColorArraysStoreLinear);
            Console.WriteLine("PASS: Material.SetColorArray in linear space stores linear colors and GetColorArray returns the originals");
            InLinear(VectorsAreNeverConverted);
            Console.WriteLine("PASS: SetVector stays unconverted in linear space on a material, a block and the globals");
            InGamma(GammaSpaceChangesNothing);
            Console.WriteLine("PASS: in gamma space a color is stored and sent as it was set");
            InLinear(PropertyBlockColorsStoreLinear);
            Console.WriteLine("PASS: MaterialPropertyBlock.SetColor and AddColor in linear space store and send linear colors");
            InLinear(GlobalColorsStoreLinear);
            Console.WriteLine("PASS: Shader.SetGlobalColor and CommandBuffer.SetGlobalColor in linear space store and send linear colors");
            InLinear(PlanColorsFollowTheRule);
            Console.WriteLine("PASS: a plan's colors are converted in linear space and its vectors are not");
        }
        finally
        {
            QualitySettings.Reset();
            DisunityHost.UnloadWorld();
        }
    }

    private static void InLinear(Action test) => In(ColorSpace.Linear, test);

    private static void InGamma(Action test) => In(ColorSpace.Gamma, test);

    private static void In(ColorSpace space, Action test)
    {
        DisunityHost.SetColorSpace(space);
        try { test(); }
        finally { QualitySettings.Reset(); }
    }

    private static Color Linear(Color color) => new(Mathf.GammaToLinearSpace(color.r), Mathf.GammaToLinearSpace(color.g), Mathf.GammaToLinearSpace(color.b), color.a);

    private static void SameColor(Color expected, Color actual, string what) => Near(expected, actual, 1e-5f, what);

    private static void SameVector(Color expected, DisunityPropertyRead read, string what)
    {
        Ensure(read.present == 4, $"{what}: the world holds a vector");
        Near(expected, new Color(read.vector_x, read.vector_y, read.vector_z, read.vector_w), 1e-5f, what);
    }

    private static Material NewMaterial(string shader) => new(DisunityHost.RegisterShader(shader));

    private static void MaterialSetColorStoresLinear()
    {
        var material = NewMaterial("Test/ColorSpace/Material");
        material.SetColor("_Tint", Authored);
        var id = Shader.PropertyToID("_TintById");
        material.SetColor(id, Authored);
        var linear = Linear(Authored);
        Ensure(Math.Abs(linear.r - 0.2462f) < 1e-3f && Math.Abs(linear.b - 0.0931f) < 1e-3f, "the expected linear value is the one a linear project needs");
        SameColor(linear, material.GetVector("_Tint"), "the stored vector is linear");
        SameColor(linear, material.GetVector(id), "the stored vector by ID is linear");
        Ensure(material.GetVector("_Tint").w == Authored.a, "alpha is not converted");
        SameColor(Authored, material.GetColor("_Tint"), "GetColor returns the color that was set");
        SameColor(Authored, material.GetColor(id), "GetColor by ID returns the color that was set");
        Ensure(material.HasColor("_Tint") && material.HasVector("_Tint"), "a color is still present as a color and a vector");
        SameVector(linear, WorldGraphicsTests.Read(MaterialScope, material.WorldLink.Id, "_Tint"), "the world material holds the linear color");
        SameVector(linear, WorldGraphicsTests.Read(MaterialScope, material.WorldLink.Id, "_TintById"), "the world material holds the linear color set by ID");
    }

    private static void MaterialColorShortcutAndCopies()
    {
        var material = NewMaterial("Test/ColorSpace/Shortcut");
        material.color = Authored;
        SameColor(Authored, material.color, "Material.color returns what was set");
        SameColor(Authored, material.GetColor("_Color"), "_Color is the color shortcut");
        SameColor(Linear(Authored), material.GetVector("_Color"), "the shortcut stores linear");
        var copy = new Material(material);
        SameColor(Authored, copy.color, "a copy keeps the color without converting again");
        var target = NewMaterial("Test/ColorSpace/Shortcut");
        target.CopyPropertiesFromMaterial(material);
        SameColor(Authored, target.color, "CopyPropertiesFromMaterial keeps the color without converting again");
        SameVector(Linear(Authored), WorldGraphicsTests.Read(MaterialScope, target.WorldLink.Id, "_Color"), "the copy sends the linear color once");
        var opaque = NewMaterial("Test/ColorSpace/Shortcut");
        opaque.color = Color.white;
        SameColor(Color.white, opaque.color, "white is white in both spaces");
    }

    private static void MaterialColorArraysStoreLinear()
    {
        var material = NewMaterial("Test/ColorSpace/Arrays");
        var colors = new[] { Authored, new Color(1f, 0.5f, 0f, 0.25f) };
        material.SetColorArray("_Palette", colors);
        material.SetColorArray(Shader.PropertyToID("_PaletteList"), new List<Color>(colors));
        var read = material.GetColorArray("_Palette")!;
        Ensure(read.Length == 2, "the array keeps its length");
        for (var i = 0; i < 2; i++) SameColor(colors[i], read[i], $"GetColorArray element {i}");
        var list = new List<Color>();
        material.GetColorArray(Shader.PropertyToID("_PaletteList"), list);
        Ensure(list.Count == 2, "the list getter keeps the length");
        for (var i = 0; i < 2; i++) SameColor(colors[i], list[i], $"GetColorArray list element {i}");
        var held = StoredArray(material, "_Palette");
        for (var i = 0; i < 2; i++) SameColor(Linear(colors[i]), held[i], $"stored element {i}");
        colors[0] = Color.black;
        SameColor(Authored, material.GetColorArray("_Palette")![0], "the store keeps its own copy");
    }

    private static Color[] StoredArray(Material material, string name)
    {
        var field = typeof(Material).GetField("_properties", System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic)!;
        var store = (PropertyStore)field.GetValue(material)!;
        return store.ColorArrays[Shader.PropertyToID(name)];
    }

    private static void VectorsAreNeverConverted()
    {
        var vector = new Vector4(0.533f, 0.051f, 0.337f, 0.6f);
        var material = NewMaterial("Test/ColorSpace/Vector");
        material.SetVector("_Vec", vector);
        Ensure(material.GetVector("_Vec") == vector, "a material vector is stored as set");
        var read = WorldGraphicsTests.Read(MaterialScope, material.WorldLink.Id, "_Vec");
        Ensure(read.present == 4 && (read.vector_x, read.vector_y, read.vector_z, read.vector_w) == (vector.x, vector.y, vector.z, vector.w), "a material vector is sent as set");
        var block = new MaterialPropertyBlock();
        block.SetVector("_Vec", vector);
        block.AddVector("_Added", vector);
        Ensure(block.GetVector("_Vec") == vector && block.GetVector("_Added") == vector, "a block vector is stored as set");
        Shader.SetGlobalVector("_ColorSpaceVec", vector);
        Ensure(Shader.GetGlobalVector("_ColorSpaceVec") == vector, "a global vector is stored as set");
        var global = WorldGraphicsTests.Read(GlobalScope, 0, "_ColorSpaceVec");
        Ensure((global.vector_x, global.vector_y, global.vector_z, global.vector_w) == (vector.x, vector.y, vector.z, vector.w), "a global vector is sent as set");
        var buffer = new CommandBuffer();
        buffer.SetGlobalVector("_ColorSpaceBufferVec", vector);
        Graphics.ExecuteCommandBuffer(buffer);
        CommandWireReader.Take();
        Ensure(Shader.GetGlobalVector("_ColorSpaceBufferVec") == vector, "a command buffer vector is stored as set");
    }

    private static void GammaSpaceChangesNothing()
    {
        var material = NewMaterial("Test/ColorSpace/Gamma");
        material.SetColor("_Tint", Authored);
        material.SetColorArray("_Palette", new[] { Authored });
        Ensure(material.GetVector("_Tint") == (Vector4)Authored && material.GetColor("_Tint") == Authored, "a material color is stored as set");
        Ensure(StoredArray(material, "_Palette")[0] == Authored && material.GetColorArray("_Palette")![0] == Authored, "a color array is stored as set");
        var read = WorldGraphicsTests.Read(MaterialScope, material.WorldLink.Id, "_Tint");
        Ensure((read.vector_x, read.vector_y, read.vector_z, read.vector_w) == (Authored.r, Authored.g, Authored.b, Authored.a), "the world material holds the color as set");
        var block = new MaterialPropertyBlock();
        block.SetColor("_Tint", Authored);
        Ensure(block.GetVector("_Tint") == (Vector4)Authored && block.GetColor("_Tint") == Authored, "a block color is stored as set");
        Shader.SetGlobalColor("_ColorSpaceGamma", Authored);
        Ensure(Shader.GetGlobalVector("_ColorSpaceGamma") == (Vector4)Authored && Shader.GetGlobalColor("_ColorSpaceGamma") == Authored, "a global color is stored as set");
    }

    private static void PropertyBlockColorsStoreLinear()
    {
        var block = new MaterialPropertyBlock();
        block.SetColor("_Tint", Authored);
        block.SetColor(Shader.PropertyToID("_TintById"), Authored);
        block.AddColor("_Added", Authored);
        block.AddColor(Shader.PropertyToID("_AddedById"), Authored);
        foreach (var name in new[] { "_Tint", "_TintById", "_Added", "_AddedById" })
        {
            SameColor(Linear(Authored), block.GetVector(name), $"{name} is stored linear");
            SameColor(Authored, block.GetColor(name), $"GetColor returns the color set as {name}");
        }
        Ensure(block.GetVector("_Tint").w == Authored.a, "alpha is not converted");
        var renderer = DisunityHost.CreateGameObject().AddComponent<MeshRenderer>();
        renderer.SetPropertyBlock(block);
        SameVector(Linear(Authored), WorldGraphicsTests.Read(BlockScope, renderer.Handle, "_Tint"), "the world block holds the linear color");
        SameVector(Linear(Authored), WorldGraphicsTests.Read(BlockScope, renderer.Handle, "_Added"), "the world block holds the linear added color");
    }

    private static void GlobalColorsStoreLinear()
    {
        Shader.SetGlobalColor("_ColorSpaceGlobal", Authored);
        Shader.SetGlobalColor(Shader.PropertyToID("_ColorSpaceGlobalById"), Authored);
        SameColor(Linear(Authored), Shader.GetGlobalVector("_ColorSpaceGlobal"), "a global color is stored linear");
        SameColor(Authored, Shader.GetGlobalColor("_ColorSpaceGlobal"), "GetGlobalColor returns the color that was set");
        SameColor(Authored, Shader.GetGlobalColor(Shader.PropertyToID("_ColorSpaceGlobalById")), "GetGlobalColor by ID returns the color that was set");
        SameVector(Linear(Authored), WorldGraphicsTests.Read(GlobalScope, 0, "_ColorSpaceGlobal"), "the world global holds the linear color");
        var buffer = new CommandBuffer();
        buffer.SetGlobalColor("_ColorSpaceBuffer", Authored);
        buffer.SetGlobalColor(Shader.PropertyToID("_ColorSpaceBufferById"), Authored);
        Graphics.ExecuteCommandBuffer(buffer);
        CommandWireReader.Take();
        SameColor(Linear(Authored), Shader.GetGlobalVector("_ColorSpaceBuffer"), "a command buffer color is stored linear, once");
        SameColor(Authored, Shader.GetGlobalColor("_ColorSpaceBuffer"), "GetGlobalColor returns the color a command buffer set");
        SameColor(Authored, Shader.GetGlobalColor("_ColorSpaceBufferById"), "GetGlobalColor returns the color a command buffer set by ID");
        SameVector(Linear(Authored), WorldGraphicsTests.Read(GlobalScope, 0, "_ColorSpaceBuffer"), "the world global holds the linear command buffer color");
    }

    private static void PlanColorsFollowTheRule()
    {
        var plan = JsonDocument.Parse("""
            {"type":"Material","name":"Planned","shader":null,"render_queue":-1,"keywords":[],"tags":[],"disabled_passes":[],
             "floats":[],"ints":[],"colors":[["_PrimaryTint",[0.533,0.051,0.337,0.6]]],"vectors":[["_Scale",[0.533,0.051,0.337,0.6]]],"textures":[]}
            """);
        var material = PlayerMaterials.Build(plan.RootElement, _ => throw new InvalidOperationException("no references"));
        var authored = new Color(0.533f, 0.051f, 0.337f, 0.6f);
        SameColor(Linear(authored), material.GetVector("_PrimaryTint"), "a plan color is stored linear");
        SameColor(authored, material.GetColor("_PrimaryTint"), "the plan color reads back as authored");
        SameVector(Linear(authored), WorldGraphicsTests.Read(MaterialScope, material.WorldLink.Id, "_PrimaryTint"), "the world holds the linear plan color");
        Ensure(material.GetVector("_Scale") == new Vector4(0.533f, 0.051f, 0.337f, 0.6f), "a plan vector is not converted");
    }
}
