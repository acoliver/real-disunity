using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Shader names, ids, globals and keywords; Material and MaterialPropertyBlock stores; the sprite materials.</summary>
internal static class ShaderMaterialTests
{
    public static void Run()
    {
        PropertyIds();
        Globals();
        GlobalArrays();
        GlobalKeywords();
        ThreadSafety();
        FindShaders();
        ShaderUnsupported();
        MaterialStore();
        MaterialArrays();
        MaterialTextures();
        MaterialConvenience();
        MaterialKeywords();
        MaterialCopyAndLerp();
        MaterialMisc();
        PropertyBlocks();
        PropertyBlockBuffersAndClear();
        PropertyBlockProbes();
        SpriteMaterials();
        Console.WriteLine("PASS: Shader ids, globals, keywords and registry; Material and MaterialPropertyBlock property stores; sprite materials stay Materials");
    }

    private static void PropertyIds()
    {
        var first = Shader.PropertyToID("_ShaderTestA");
        Ensure(first >= 1000 && Shader.PropertyToID("_ShaderTestA") == first, "the same name gives the same ID");
        Ensure(Shader.PropertyToID("_ShaderTestB") != first && Shader.PropertyToID("_shadertesta") != first, "different names give different IDs, case included");
        var ids = Enumerable.Range(0, 200).Select(i => Shader.PropertyToID($"_ShaderTestMany{i}")).ToArray();
        Ensure(ids.Distinct().Count() == 200 && ids.SequenceEqual(Enumerable.Range(0, 200).Select(i => Shader.PropertyToID($"_ShaderTestMany{i}"))), "many names stay distinct and stable");
        Ensure(Shader.PropertyToID("") != first, "the empty name has an ID");
        Throws<ArgumentNullException>(() => Shader.PropertyToID(null!), "a null name");
        var threaded = new int[8];
        Parallel.For(0, 8, i => threaded[i] = Shader.PropertyToID("_ShaderTestThreads"));
        Ensure(threaded.Distinct().Count() == 1, "IDs are stable across threads");
    }

    private static void Globals()
    {
        Shader.SetGlobalFloat("_GlobalF", 1.5f);
        Ensure(Shader.GetGlobalFloat("_GlobalF") == 1.5f && Shader.GetGlobalFloat(Shader.PropertyToID("_GlobalF")) == 1.5f, "a global float by name and by ID");
        Shader.SetGlobalFloat(Shader.PropertyToID("_GlobalF2"), 2.5f);
        Ensure(Shader.GetGlobalFloat("_GlobalF2") == 2.5f, "set by ID, read by name");
        Shader.SetGlobalInt("_GlobalI", 7);
        Ensure(Shader.GetGlobalInt("_GlobalI") == 7 && Shader.GetGlobalFloat("_GlobalI") == 7f && Shader.GetGlobalInteger("_GlobalI") == 0, "SetGlobalInt stores a float, as Unity does");
        Shader.SetGlobalInteger("_GlobalInteger", 9);
        Ensure(Shader.GetGlobalInteger("_GlobalInteger") == 9 && Shader.GetGlobalFloat("_GlobalInteger") == 0f, "SetGlobalInteger stores an integer");
        Shader.SetGlobalInteger(Shader.PropertyToID("_GlobalInteger2"), -4);
        Shader.SetGlobalInt(Shader.PropertyToID("_GlobalI2"), 8);
        Ensure(Shader.GetGlobalInteger(Shader.PropertyToID("_GlobalInteger2")) == -4 && Shader.GetGlobalInt(Shader.PropertyToID("_GlobalI2")) == 8, "ints by ID");
        Shader.SetGlobalVector("_GlobalV", new Vector4(1, 2, 3, 4));
        Ensure(Shader.GetGlobalVector("_GlobalV") == new Vector4(1, 2, 3, 4) && Shader.GetGlobalVector(Shader.PropertyToID("_GlobalV")) == new Vector4(1, 2, 3, 4), "a global vector");
        Shader.SetGlobalColor("_GlobalC", new Color(0.1f, 0.2f, 0.3f, 0.4f));
        Ensure(Shader.GetGlobalColor("_GlobalC") == new Color(0.1f, 0.2f, 0.3f, 0.4f) && Shader.GetGlobalVector("_GlobalC") == new Vector4(0.1f, 0.2f, 0.3f, 0.4f), "a color shares the vector slot");
        Shader.SetGlobalColor(Shader.PropertyToID("_GlobalC2"), Color.red);
        Shader.SetGlobalVector(Shader.PropertyToID("_GlobalV2"), Vector4.one);
        Ensure(Shader.GetGlobalColor(Shader.PropertyToID("_GlobalC2")) == Color.red && Shader.GetGlobalColor("_GlobalV2") == Color.white, "colors and vectors by ID");
        var matrix = Matrix4x4.Scale(new Vector3(2, 3, 4));
        Shader.SetGlobalMatrix("_GlobalM", matrix);
        Ensure(Shader.GetGlobalMatrix("_GlobalM") == matrix && Shader.GetGlobalMatrix(Shader.PropertyToID("_GlobalM")) == matrix, "a global matrix");
        Shader.SetGlobalMatrix(Shader.PropertyToID("_GlobalM2"), matrix);
        Ensure(Shader.GetGlobalMatrix("_GlobalM2") == matrix && Shader.GetGlobalMatrix("_GlobalNeverSet") == Matrix4x4.identity, "an unset matrix is the identity");
        var texture = new Texture2D(2, 2);
        Shader.SetGlobalTexture("_GlobalT", texture);
        Ensure(ReferenceEquals(Shader.GetGlobalTexture("_GlobalT"), texture) && ReferenceEquals(Shader.GetGlobalTexture(Shader.PropertyToID("_GlobalT")), texture), "a global texture");
        var target = new RenderTexture(8, 8, 0);
        Shader.SetGlobalTexture("_GlobalRT", target, RenderTextureSubElement.Depth);
        Shader.SetGlobalTexture(Shader.PropertyToID("_GlobalRT2"), target, RenderTextureSubElement.Color);
        Shader.SetGlobalTexture(Shader.PropertyToID("_GlobalT2"), texture);
        Ensure(ReferenceEquals(Shader.GetGlobalTexture("_GlobalRT"), target) && ReferenceEquals(Shader.GetGlobalTexture("_GlobalRT2"), target) && ReferenceEquals(Shader.GetGlobalTexture("_GlobalT2"), texture), "a global render texture sub-element");
        Shader.SetGlobalTexture("_GlobalT", null!);
        Ensure(Shader.GetGlobalTexture("_GlobalT") == null, "a texture can be unset");
        Ensure(Shader.GetGlobalFloat("_GlobalNeverSet") == 0f && Shader.GetGlobalVector("_GlobalNeverSet") == Vector4.zero && Shader.GetGlobalTexture("_GlobalNeverSet") == null && Shader.GetGlobalInteger("_GlobalNeverSet") == 0, "unset globals read as zero");
        Shader.SetGlobalBuffer("_GlobalB", (ComputeBuffer)null!);
        Shader.SetGlobalBuffer(Shader.PropertyToID("_GlobalB2"), (GraphicsBuffer)null!);
        Shader.SetGlobalBuffer("_GlobalB3", (GraphicsBuffer)null!);
        Shader.SetGlobalBuffer(Shader.PropertyToID("_GlobalB4"), (ComputeBuffer)null!);
        Shader.SetGlobalConstantBuffer("_GlobalCB", (ComputeBuffer)null!, 0, 16);
        Shader.SetGlobalConstantBuffer("_GlobalCB2", (GraphicsBuffer)null!, 0, 16);
        Shader.SetGlobalConstantBuffer(Shader.PropertyToID("_GlobalCB3"), (ComputeBuffer)null!, 0, 16);
        Shader.SetGlobalConstantBuffer(Shader.PropertyToID("_GlobalCB4"), (GraphicsBuffer)null!, 0, 16);
        Shader.SetGlobalRayTracingAccelerationStructure("_GlobalRT", (RayTracingAccelerationStructure)null!);
        Shader.SetGlobalRayTracingAccelerationStructure(Shader.PropertyToID("_GlobalRT3"), (RayTracingAccelerationStructure)null!);
    }

    private static void GlobalArrays()
    {
        var floats = new[] { 1f, 2f, 3f };
        Shader.SetGlobalFloatArray("_GlobalFA", floats);
        floats[0] = 99f;
        Ensure(Shader.GetGlobalFloatArray("_GlobalFA")!.SequenceEqual(new[] { 1f, 2f, 3f }), "a stored array is a copy");
        var read = Shader.GetGlobalFloatArray(Shader.PropertyToID("_GlobalFA"))!;
        read[1] = -1f;
        Ensure(Shader.GetGlobalFloatArray("_GlobalFA")![1] == 2f, "a read array is a copy");
        var list = new List<float> { 9f };
        Shader.GetGlobalFloatArray("_GlobalFA", list);
        Ensure(list.SequenceEqual(new[] { 1f, 2f, 3f }), "a list is filled");
        Shader.GetGlobalFloatArray(Shader.PropertyToID("_GlobalNeverSetArray"), list);
        Ensure(list.Count == 0 && Shader.GetGlobalFloatArray("_GlobalNeverSetArray") == null, "a missing array empties the list and reads null");
        Shader.SetGlobalFloatArray(Shader.PropertyToID("_GlobalFA2"), new List<float> { 4f, 5f });
        Shader.SetGlobalFloatArray("_GlobalFA3", new List<float> { 6f });
        Shader.SetGlobalFloatArray(Shader.PropertyToID("_GlobalFA4"), new[] { 7f });
        Ensure(Shader.GetGlobalFloatArray("_GlobalFA2")!.SequenceEqual(new[] { 4f, 5f }) && Shader.GetGlobalFloatArray("_GlobalFA3")![0] == 6f && Shader.GetGlobalFloatArray("_GlobalFA4")![0] == 7f, "float array overloads");

        var vectors = new[] { Vector4.one, Vector4.zero };
        Shader.SetGlobalVectorArray("_GlobalVA", vectors);
        Shader.SetGlobalVectorArray(Shader.PropertyToID("_GlobalVA2"), new List<Vector4> { Vector4.one });
        Shader.SetGlobalVectorArray("_GlobalVA3", new List<Vector4> { Vector4.one });
        Shader.SetGlobalVectorArray(Shader.PropertyToID("_GlobalVA4"), vectors);
        var vectorList = new List<Vector4>();
        Shader.GetGlobalVectorArray("_GlobalVA", vectorList);
        Shader.GetGlobalVectorArray(Shader.PropertyToID("_GlobalVA"), vectorList);
        Ensure(vectorList.Count == 2 && Shader.GetGlobalVectorArray("_GlobalVA")![0] == Vector4.one && Shader.GetGlobalVectorArray(Shader.PropertyToID("_GlobalVA4"))!.Length == 2, "vector arrays");
        var matrices = new[] { Matrix4x4.identity, Matrix4x4.zero };
        Shader.SetGlobalMatrixArray("_GlobalMA", matrices);
        Shader.SetGlobalMatrixArray(Shader.PropertyToID("_GlobalMA2"), new List<Matrix4x4> { Matrix4x4.identity });
        Shader.SetGlobalMatrixArray("_GlobalMA3", new List<Matrix4x4> { Matrix4x4.identity });
        Shader.SetGlobalMatrixArray(Shader.PropertyToID("_GlobalMA4"), matrices);
        var matrixList = new List<Matrix4x4>();
        Shader.GetGlobalMatrixArray("_GlobalMA", matrixList);
        Shader.GetGlobalMatrixArray(Shader.PropertyToID("_GlobalMA"), matrixList);
        Ensure(matrixList.Count == 2 && Shader.GetGlobalMatrixArray("_GlobalMA")![1] == Matrix4x4.zero && Shader.GetGlobalMatrixArray(Shader.PropertyToID("_GlobalMA4"))!.Length == 2, "matrix arrays");

        Throws<ArgumentException>(() => Shader.SetGlobalFloatArray("_GlobalEmpty", new float[0]), "an empty array", "at least one element");
        Throws<ArgumentException>(() => Shader.SetGlobalFloatArray("_GlobalBig", new float[1024]), "1024 elements", "1023");
        Shader.SetGlobalFloatArray("_GlobalMax", new float[1023]);
        Throws<ArgumentNullException>(() => Shader.SetGlobalVectorArray("_GlobalNull", (Vector4[])null!), "a null array");
        Throws<ArgumentNullException>(() => Shader.SetGlobalFloatArray("_GlobalNull", (List<float>)null!), "a null list");
        Throws<ArgumentNullException>(() => Shader.GetGlobalFloatArray("_GlobalFA", null!), "a null destination list");
    }

    private static void GlobalKeywords()
    {
        Ensure(!Shader.IsKeywordEnabled("_SHADER_TEST_A"), "a keyword starts disabled");
        Shader.EnableKeyword("_SHADER_TEST_A");
        Ensure(Shader.IsKeywordEnabled("_SHADER_TEST_A") && Shader.enabledGlobalKeywords.Any(keyword => keyword.name == "_SHADER_TEST_A"), "EnableKeyword");
        Ensure(Shader.globalKeywords.Any(keyword => keyword.name == "_SHADER_TEST_A"), "an enabled keyword is a known global keyword");
        Shader.DisableKeyword("_SHADER_TEST_A");
        Ensure(!Shader.IsKeywordEnabled("_SHADER_TEST_A") && Shader.enabledGlobalKeywords.All(keyword => keyword.name != "_SHADER_TEST_A"), "DisableKeyword");
        Ensure(Shader.globalKeywords.Any(keyword => keyword.name == "_SHADER_TEST_A"), "a disabled keyword stays known");
        var keyword = GlobalKeyword.Create("_SHADER_TEST_B");
        Ensure(keyword.name == "_SHADER_TEST_B" && keyword.ToString() == "_SHADER_TEST_B" && new GlobalKeyword("_SHADER_TEST_B").name == keyword.name, "GlobalKeyword");
        Ensure(Shader.globalKeywords.Any(known => known.name == "_SHADER_TEST_B") && !Shader.IsKeywordEnabled(ref keyword), "creating a keyword does not enable it");
        Shader.EnableKeyword(ref keyword);
        Ensure(Shader.IsKeywordEnabled(ref keyword) && Shader.IsKeywordEnabled("_SHADER_TEST_B"), "EnableKeyword(ref)");
        Shader.DisableKeyword(ref keyword);
        Ensure(!Shader.IsKeywordEnabled(ref keyword), "DisableKeyword(ref)");
        Shader.SetKeyword(ref keyword, true);
        Ensure(Shader.IsKeywordEnabled("_SHADER_TEST_B"), "SetKeyword true");
        Shader.SetKeyword(ref keyword, false);
        Ensure(!Shader.IsKeywordEnabled("_SHADER_TEST_B"), "SetKeyword false");
        Shader.EnableKeyword("_SHADER_TEST_C");
        Shader.EnableKeyword("_SHADER_TEST_C");
        Ensure(Shader.enabledGlobalKeywords.Count(known => known.name == "_SHADER_TEST_C") == 1, "enabling twice lists once");
        Shader.DisableKeyword("_SHADER_TEST_C");
        Shader.DisableKeyword("_SHADER_TEST_NEVER");
        Throws<ArgumentNullException>(() => Shader.EnableKeyword((string)null!), "a null keyword");
        Throws<ArgumentNullException>(() => new GlobalKeyword(null!), "a null GlobalKeyword name");
        Ensure(default(GlobalKeyword).name == "", "a default keyword has an empty name");
    }

    private static void ThreadSafety()
    {
        Parallel.For(0, 64, i =>
        {
            _ = new GlobalKeyword("_ShaderTestPar" + i % 8);
            Shader.EnableKeyword("_ShaderTestParOn" + i % 8);
            _ = Shader.globalKeywords;
        });
        Ensure(Shader.globalKeywords.Count(keyword => keyword.name.StartsWith("_ShaderTestPar")) == 16, "concurrent keyword use keeps one entry per keyword");
        Ensure(Shader.enabledGlobalKeywords.Count(keyword => keyword.name.StartsWith("_ShaderTestParOn")) == 8, "concurrent enabling keeps one entry per keyword");

        foreach (var (what, action) in new (string, Action)[]
        {
            ("Shader.SetGlobalFloat", () => Shader.SetGlobalFloat("_ShaderTestOff", 1f)),
            ("Shader.GetGlobalFloat", () => Shader.GetGlobalFloat("_ShaderTestOff")),
            ("RenderTexture.active", () => _ = RenderTexture.active),
            ("RenderTexture.GetTemporary", () => RenderTexture.GetTemporary(4, 4)),
            ("Texture2D.whiteTexture", () => _ = Texture2D.whiteTexture),
        })
        {
            var failure = OnOtherThread(action);
            Ensure(failure is InvalidOperationException && failure.Message.Contains("main thread"), $"{what} fails off the main thread (got {failure?.GetType().Name ?? "no error"})");
        }
        Shader.SetGlobalFloat("_ShaderTestOff", 2f);
        Ensure(Shader.GetGlobalFloat("_ShaderTestOff") == 2f, "globals work on the main thread");
    }

    private static void FindShaders()
    {
        Ensure(Shader.Find("Test/ShaderMaterial/Missing") == null, "an unknown shader is not found");
        var shader = DisunityHost.RegisterShader("Test/ShaderMaterial/Lit");
        Ensure(shader.name == "Test/ShaderMaterial/Lit" && ReferenceEquals(Shader.Find("Test/ShaderMaterial/Lit"), shader), "a registered shader is found");
        Ensure(ReferenceEquals(DisunityHost.RegisterShader("Test/ShaderMaterial/Lit"), shader) && ReferenceEquals(Shader.Find("test/shadermaterial/lit"), shader), "registering again returns the same shader; names ignore case");
        Ensure(shader.isSupported, "a registered shader is supported");
        NotSupported(() => _ = shader.renderQueue, "an undeclared shader's queue", "Shader.renderQueue");
        NotSupported(() => _ = shader.passCount, "an undeclared shader's pass count", "Shader.passCount");
        NotSupported(() => _ = shader.subshaderCount, "an undeclared shader's subshader count", "Shader.subshaderCount");
        NotSupported(() => _ = shader.maximumLOD, "an undeclared shader's LOD", "Shader.maximumLOD");
        NotSupported(() => shader.GetPassCountInSubshader(0), "pass count in a subshader", "Shader.passCount");
        Throws<ArgumentOutOfRangeException>(() => shader.GetPassCountInSubshader(1), "a second subshader", "subshaderIndex");
        Throws<ArgumentNullException>(() => Shader.Find(null!), "Find(null)");
        Throws<ArgumentNullException>(() => DisunityHost.RegisterShader(null!), "RegisterShader(null)");
        shader.maximumLOD = 300;
        Ensure(shader.maximumLOD == 300, "maximumLOD is stored");
        var (lod, pipeline, tier, chunks) = (Shader.globalMaximumLOD, Shader.globalRenderPipeline, Shader.globalShaderHardwareTier, Shader.maximumChunksOverride);
        Shader.globalMaximumLOD = 200;
        Shader.globalRenderPipeline = "UniversalPipeline";
        Shader.globalShaderHardwareTier = ShaderHardwareTier.Tier1;
        Shader.maximumChunksOverride = 4;
        Ensure(Shader.globalMaximumLOD == 200 && Shader.globalRenderPipeline == "UniversalPipeline" && Shader.globalShaderHardwareTier == ShaderHardwareTier.Tier1 && Shader.maximumChunksOverride == 4, "global shader settings are stored");
        (Shader.globalMaximumLOD, Shader.globalRenderPipeline, Shader.globalShaderHardwareTier, Shader.maximumChunksOverride) = (lod, pipeline, tier, chunks);
        Shader.WarmupAllShaders();
        Ensure(ShaderTagId.none != new ShaderTagId("Opaque") && new ShaderTagId("Opaque") == new ShaderTagId("Opaque") && new ShaderTagId("Opaque").name == "Opaque"
            && (ShaderTagId)"Opaque" == new ShaderTagId("Opaque") && (string)new ShaderTagId("Opaque") == "Opaque" && ShaderTagId.none.name == "", "ShaderTagId names");
    }

    private static void ShaderUnsupported()
    {
        var shader = DisunityHost.RegisterShader("Test/ShaderMaterial/Unsupported");
        var stack = "";
        var layer = 0;
        var calls = new (string Member, Action Call)[]
        {
            ("GetPropertyCount", () => shader.GetPropertyCount()), ("FindPropertyIndex", () => shader.FindPropertyIndex("_X")),
            ("GetPropertyName", () => shader.GetPropertyName(0)), ("GetPropertyNameId", () => shader.GetPropertyNameId(0)),
            ("GetPropertyType", () => shader.GetPropertyType(0)), ("GetPropertyDescription", () => shader.GetPropertyDescription(0)),
            ("GetPropertyFlags", () => shader.GetPropertyFlags(0)), ("GetPropertyAttributes", () => shader.GetPropertyAttributes(0)),
            ("GetPropertyDefaultFloatValue", () => shader.GetPropertyDefaultFloatValue(0)), ("GetPropertyDefaultVectorValue", () => shader.GetPropertyDefaultVectorValue(0)),
            ("GetPropertyRangeLimits", () => shader.GetPropertyRangeLimits(0)), ("GetPropertyDefaultIntValue", () => shader.GetPropertyDefaultIntValue(0)),
            ("GetPropertyTextureDimension", () => shader.GetPropertyTextureDimension(0)), ("GetPropertyTextureDefaultName", () => shader.GetPropertyTextureDefaultName(0)),
            ("FindTextureStack", () => shader.FindTextureStack(0, ref stack, ref layer)), ("keywordSpace", () => _ = shader.keywordSpace),
            ("GetDependency", () => shader.GetDependency("x")), ("FindPassTagValue", () => shader.FindPassTagValue(0, ShaderTagId.none)),
            ("FindPassTagValue", () => shader.FindPassTagValue(0, 0, ShaderTagId.none)), ("FindSubshaderTagValue", () => shader.FindSubshaderTagValue(0, ShaderTagId.none)),
        };
        foreach (var (member, call) in calls) NotSupported(call, member, $"Shader.{member}");
        var space = default(LocalKeywordSpace);
        NotSupported(() => _ = space.keywords, "keywords", "LocalKeywordSpace.keywords");
        NotSupported(() => _ = space.keywordNames, "keywordNames", "LocalKeywordSpace.keywordNames");
        NotSupported(() => _ = space.keywordCount, "keywordCount", "LocalKeywordSpace.keywordCount");
        NotSupported(() => space.FindKeyword("x"), "FindKeyword", "LocalKeywordSpace.FindKeyword");
        Ensure(space == default && space.Equals(default(LocalKeywordSpace)) && !(space != default) && space.Equals((object)default(LocalKeywordSpace)) && space.GetHashCode() == 0, "a default keyword space equals itself");
    }

    private static void MaterialStore()
    {
        var shader = DisunityHost.RegisterShader("Test/ShaderMaterial/Store");
        var material = new Material(shader);
        Ensure(ReferenceEquals(material.shader, shader) && !material.HasProperty("_Cutoff") && !material.HasFloat("_Cutoff"), "a new material has no properties set");
        material.SetFloat("_Cutoff", 0.25f);
        Ensure(material.GetFloat("_Cutoff") == 0.25f && material.GetFloat(Shader.PropertyToID("_Cutoff")) == 0.25f && material.HasProperty("_Cutoff") && material.HasFloat("_Cutoff"), "a float by name and ID");
        Ensure(material.HasProperty(Shader.PropertyToID("_Cutoff")) && material.HasFloat(Shader.PropertyToID("_Cutoff")) && !material.HasVector("_Cutoff") && !material.HasTexture("_Cutoff"), "HasProperty and the typed Has methods");
        material.SetInt("_Mode", 3);
        Ensure(material.GetInt("_Mode") == 3 && material.GetFloat("_Mode") == 3f && material.HasInt("_Mode") && material.HasFloat("_Mode") && !material.HasInteger("_Mode") && material.GetInteger("_Mode") == 0, "SetInt stores a float");
        material.SetInteger("_Count", 12);
        Ensure(material.GetInteger("_Count") == 12 && material.HasInteger("_Count") && !material.HasInt("_Count") && material.GetFloat("_Count") == 0f, "SetInteger stores an integer");
        material.SetInteger(Shader.PropertyToID("_Count2"), 5);
        material.SetInt(Shader.PropertyToID("_Mode2"), 6);
        Ensure(material.GetInteger(Shader.PropertyToID("_Count2")) == 5 && material.GetInt(Shader.PropertyToID("_Mode2")) == 6 && material.HasInteger(Shader.PropertyToID("_Count2")) && material.HasInt(Shader.PropertyToID("_Mode2")), "ints by ID");
        material.SetVector("_Tiling", new Vector4(1, 2, 3, 4));
        Ensure(material.GetVector("_Tiling") == new Vector4(1, 2, 3, 4) && material.HasVector("_Tiling") && material.HasColor("_Tiling"), "a vector, and a color is a vector");
        material.SetColor("_Tint", new Color(0.1f, 0.2f, 0.3f, 0.4f));
        Ensure(material.GetColor("_Tint") == new Color(0.1f, 0.2f, 0.3f, 0.4f) && material.GetVector(Shader.PropertyToID("_Tint")) == new Vector4(0.1f, 0.2f, 0.3f, 0.4f) && material.HasColor(Shader.PropertyToID("_Tint")), "a color");
        material.SetColor(Shader.PropertyToID("_Tint2"), Color.green);
        material.SetVector(Shader.PropertyToID("_Tiling2"), Vector4.one);
        Ensure(material.GetColor(Shader.PropertyToID("_Tint2")) == Color.green && material.GetVector("_Tiling2") == Vector4.one && material.HasVector(Shader.PropertyToID("_Tiling2")), "colors and vectors by ID");
        var matrix = Matrix4x4.Translate(new Vector3(1, 2, 3));
        material.SetMatrix("_Transform", matrix);
        material.SetMatrix(Shader.PropertyToID("_Transform2"), matrix);
        Ensure(material.GetMatrix("_Transform") == matrix && material.GetMatrix(Shader.PropertyToID("_Transform2")) == matrix && material.HasMatrix("_Transform") && material.HasMatrix(Shader.PropertyToID("_Transform2")), "a matrix");
        Ensure(material.GetMatrix("_NeverSet") == Matrix4x4.identity && material.GetFloat("_NeverSet") == 0f && material.GetColor("_NeverSet") == default && material.GetTexture("_NeverSet") == null, "unset properties read as zero values");
        material.SetFloat("_Cutoff", 0.75f);
        Ensure(material.GetFloat("_Cutoff") == 0.75f, "a later set replaces the value");

        var second = new Material(shader);
        Ensure(!second.HasProperty("_Cutoff") && second.GetFloat("_Cutoff") == 0f, "materials do not share values");
        Ensure(!new MaterialPropertyBlock().HasProperty("_Cutoff"), "a block does not share values with a material");
        var buffers = new Material(shader);
        buffers.SetBuffer("_Buf", (ComputeBuffer)null!);
        buffers.SetBuffer(Shader.PropertyToID("_Buf2"), (GraphicsBuffer)null!);
        buffers.SetBuffer("_Buf3", (GraphicsBuffer)null!);
        buffers.SetBuffer(Shader.PropertyToID("_Buf4"), (ComputeBuffer)null!);
        buffers.SetConstantBuffer("_Const", (ComputeBuffer)null!, 0, 64);
        buffers.SetConstantBuffer("_Const2", (GraphicsBuffer)null!, 0, 64);
        buffers.SetConstantBuffer(Shader.PropertyToID("_Const3"), (ComputeBuffer)null!, 0, 64);
        buffers.SetConstantBuffer(Shader.PropertyToID("_Const4"), (GraphicsBuffer)null!, 16, 64);
        Ensure(buffers.HasBuffer("_Buf") && buffers.HasBuffer(Shader.PropertyToID("_Buf2")) && buffers.HasConstantBuffer("_Const") && buffers.HasConstantBuffer(Shader.PropertyToID("_Const4"))
            && !buffers.HasBuffer("_Const") && !buffers.HasConstantBuffer("_Buf"), "buffers are bound by kind");
        NotSupported(() => buffers.GetBuffer("_Buf"), "GetBuffer", "Material.GetBuffer");
        NotSupported(() => buffers.GetConstantBuffer("_Const"), "GetConstantBuffer", "Material.GetConstantBuffer");
        Throws<ArgumentNullException>(() => new Material((Shader)null!), "a material of no shader");
        Throws<ArgumentNullException>(() => new Material((Material)null!), "a copy of no material");
    }

    private static void MaterialArrays()
    {
        var material = new Material(DisunityHost.RegisterShader("Test/ShaderMaterial/Arrays"));
        material.SetFloatArray("_F", new[] { 1f, 2f });
        material.SetFloatArray(Shader.PropertyToID("_F2"), new List<float> { 3f });
        material.SetFloatArray("_F3", new List<float> { 3f });
        material.SetFloatArray(Shader.PropertyToID("_F4"), new[] { 3f });
        material.SetVectorArray("_V", new[] { Vector4.one });
        material.SetVectorArray(Shader.PropertyToID("_V2"), new List<Vector4> { Vector4.zero });
        material.SetVectorArray("_V3", new List<Vector4> { Vector4.zero });
        material.SetVectorArray(Shader.PropertyToID("_V4"), new[] { Vector4.zero });
        material.SetMatrixArray("_M", new[] { Matrix4x4.identity });
        material.SetMatrixArray(Shader.PropertyToID("_M2"), new List<Matrix4x4> { Matrix4x4.zero });
        material.SetMatrixArray("_M3", new List<Matrix4x4> { Matrix4x4.zero });
        material.SetMatrixArray(Shader.PropertyToID("_M4"), new[] { Matrix4x4.zero });
        material.SetColorArray("_C", new[] { Color.red, Color.blue });
        material.SetColorArray(Shader.PropertyToID("_C2"), new List<Color> { Color.red });
        material.SetColorArray("_C3", new List<Color> { Color.red });
        material.SetColorArray(Shader.PropertyToID("_C4"), new[] { Color.red });
        Ensure(material.GetFloatArray("_F")!.SequenceEqual(new[] { 1f, 2f }) && material.GetFloatArray(Shader.PropertyToID("_F2"))![0] == 3f, "float arrays");
        Ensure(material.GetVectorArray("_V")![0] == Vector4.one && material.GetVectorArray(Shader.PropertyToID("_V2"))!.Length == 1 && material.GetMatrixArray("_M")![0] == Matrix4x4.identity && material.GetMatrixArray(Shader.PropertyToID("_M2"))![0] == Matrix4x4.zero, "vector and matrix arrays");
        Ensure(material.GetColorArray("_C")!.SequenceEqual(new[] { Color.red, Color.blue }) && material.GetColorArray(Shader.PropertyToID("_C2"))!.Length == 1, "color arrays");
        var (floats, colors, vectors, matrices) = (new List<float>(), new List<Color>(), new List<Vector4>(), new List<Matrix4x4>());
        material.GetFloatArray("_F", floats);
        material.GetFloatArray(Shader.PropertyToID("_F"), floats);
        material.GetColorArray("_C", colors);
        material.GetColorArray(Shader.PropertyToID("_C"), colors);
        material.GetVectorArray("_V", vectors);
        material.GetVectorArray(Shader.PropertyToID("_V"), vectors);
        material.GetMatrixArray("_M", matrices);
        material.GetMatrixArray(Shader.PropertyToID("_M"), matrices);
        Ensure(floats.Count == 2 && colors.Count == 2 && vectors.Count == 1 && matrices.Count == 1 && material.GetFloatArray("_Missing") == null && material.GetColorArray("_Missing") == null, "list getters and missing arrays");
        Ensure(material.HasProperty("_F") && material.HasProperty("_C") && material.HasProperty("_M") && !material.HasFloat("_F"), "an array makes the property exist, but not as a float");
        Throws<ArgumentException>(() => material.SetColorArray("_E", new Color[0]), "an empty color array", "SetColorArray");
        Throws<ArgumentException>(() => material.SetVectorArray("_E", new Vector4[1024]), "an oversized vector array", "SetVectorArray");
    }

    private static void MaterialTextures()
    {
        var material = new Material(DisunityHost.RegisterShader("Test/ShaderMaterial/Textures"));
        var texture = new Texture2D(2, 2);
        var target = new RenderTexture(8, 8, 0);
        material.SetTexture("_MainTex", texture);
        material.SetTexture(Shader.PropertyToID("_Detail"), target);
        material.SetTexture("_Depth", target, RenderTextureSubElement.Depth);
        material.SetTexture(Shader.PropertyToID("_Depth2"), target, RenderTextureSubElement.Stencil);
        Ensure(ReferenceEquals(material.GetTexture("_MainTex"), texture) && ReferenceEquals(material.GetTexture(Shader.PropertyToID("_Detail")), target) && ReferenceEquals(material.GetTexture("_Depth"), target) && ReferenceEquals(material.GetTexture(Shader.PropertyToID("_Depth2")), target), "textures");
        Ensure(material.HasTexture("_MainTex") && material.HasTexture(Shader.PropertyToID("_Detail")) && !material.HasFloat("_MainTex"), "HasTexture");
        Ensure(material.GetTexturePropertyNames().OrderBy(name => name).SequenceEqual(new[] { "_Depth", "_Depth2", "_Detail", "_MainTex" }), "texture property names");
        Ensure(material.GetTexturePropertyNameIDs().Length == 4 && material.GetTexturePropertyNameIDs().Contains(Shader.PropertyToID("_MainTex")), "texture property IDs");
        var names = new List<string> { "old" };
        var ids = new List<int> { -1 };
        material.GetTexturePropertyNames(names);
        material.GetTexturePropertyNameIDs(ids);
        Ensure(names.Count == 4 && !names.Contains("old") && ids.Count == 4 && !ids.Contains(-1), "the list forms replace the list's contents");
        material.SetFloat("_Cut", 1f);
        material.SetInteger("_Int", 1);
        material.SetVector("_Vec", Vector4.one);
        material.SetMatrix("_Mat", Matrix4x4.identity);
        material.SetConstantBuffer("_CB", (ComputeBuffer)null!, 0, 1);
        material.SetBuffer("_CBuf", (ComputeBuffer)null!);
        Ensure(material.GetPropertyNames(MaterialPropertyType.Float).Contains("_Cut") && material.GetPropertyNames(MaterialPropertyType.Int).Contains("_Int")
            && material.GetPropertyNames(MaterialPropertyType.Vector).Contains("_Vec") && material.GetPropertyNames(MaterialPropertyType.Matrix).Contains("_Mat")
            && material.GetPropertyNames(MaterialPropertyType.Texture).Contains("_MainTex") && material.GetPropertyNames(MaterialPropertyType.ConstantBuffer).Contains("_CB")
            && material.GetPropertyNames(MaterialPropertyType.ComputeBuffer).Contains("_CBuf"), "GetPropertyNames by type");
        Throws<ArgumentOutOfRangeException>(() => material.GetPropertyNames((MaterialPropertyType)99), "an unknown property type");
    }

    private static void MaterialConvenience()
    {
        var material = new Material(DisunityHost.RegisterShader("Test/ShaderMaterial/Convenience"));
        material.color = new Color(0.5f, 0.25f, 0.125f, 1f);
        Ensure(material.GetColor("_Color") == new Color(0.5f, 0.25f, 0.125f, 1f) && material.HasColor("_Color") && material.color == material.GetColor("_Color"), "color is _Color");
        var texture = new Texture2D(2, 2);
        Ensure(material.mainTexture == null && material.mainTextureScale == Vector2.one && material.mainTextureOffset == Vector2.zero, "main texture defaults");
        material.mainTexture = texture;
        material.mainTextureOffset = new Vector2(0.5f, 0.25f);
        material.mainTextureScale = new Vector2(2, 3);
        Ensure(ReferenceEquals(material.GetTexture("_MainTex"), texture) && ReferenceEquals(material.mainTexture, texture) && material.HasTexture("_MainTex"), "mainTexture is _MainTex");
        Ensure(material.GetTextureOffset("_MainTex") == new Vector2(0.5f, 0.25f) && material.GetTextureScale("_MainTex") == new Vector2(2, 3), "offset and scale are _MainTex's");
        material.SetTextureOffset("_Other", new Vector2(1, 1));
        material.SetTextureScale(Shader.PropertyToID("_Other"), new Vector2(4, 4));
        material.SetTextureOffset(Shader.PropertyToID("_Other2"), new Vector2(2, 2));
        material.SetTextureScale("_Other2", new Vector2(5, 5));
        Ensure(material.GetTextureOffset(Shader.PropertyToID("_Other")) == new Vector2(1, 1) && material.GetTextureScale("_Other") == new Vector2(4, 4) && material.GetTextureOffset("_Other2") == new Vector2(2, 2) && material.GetTextureScale(Shader.PropertyToID("_Other2")) == new Vector2(5, 5), "offset and scale by name and ID");
        Ensure(material.HasProperty("_Other") && !material.HasTexture("_Other"), "an offset makes the property exist, but not as a texture");
        material.mainTexture = null;
        Ensure(material.mainTexture == null && material.HasTexture("_MainTex"), "an unset main texture still counts as set");

        var shader = material.shader;
        NotSupported(() => _ = material.renderQueue, "an undeclared shader's queue", "Shader.renderQueue");
        NotSupported(() => _ = material.passCount, "an undeclared shader's pass count", "Shader.passCount");
        var sprite = new Material(Material.SpriteUnlit.shader);
        Ensure(sprite.renderQueue == 3000 && sprite.rawRenderQueue == -1 && sprite.passCount == 1, "the queue comes from the shader by default");
        material.renderQueue = 2450;
        Ensure(material.renderQueue == 2450 && material.rawRenderQueue == 2450, "a stored render queue");
        material.renderQueue = -1;
        Ensure(material.rawRenderQueue == -1 && sprite.renderQueue == sprite.shader.renderQueue, "-1 returns to the shader's queue");
        material.globalIlluminationFlags = MaterialGlobalIlluminationFlags.BakedEmissive;
        material.enableInstancing = true;
        material.doubleSidedGI = true;
        Ensure(material.globalIlluminationFlags == MaterialGlobalIlluminationFlags.BakedEmissive && material.enableInstancing && material.doubleSidedGI, "stored flags");
        var other = DisunityHost.RegisterShader("Test/ShaderMaterial/Other");
        material.shader = other;
        Ensure(ReferenceEquals(material.shader, other), "the shader can change");
    }

    private static void MaterialKeywords()
    {
        var shader = DisunityHost.RegisterShader("Test/ShaderMaterial/Keywords");
        var material = new Material(shader);
        Ensure(material.shaderKeywords.Length == 0 && !material.IsKeywordEnabled("_A"), "no keywords at first");
        material.EnableKeyword("_A");
        material.EnableKeyword("_B");
        material.EnableKeyword("_A");
        Ensure(material.shaderKeywords.SequenceEqual(new[] { "_A", "_B" }) && material.IsKeywordEnabled("_A"), "enabled keywords, once each");
        material.DisableKeyword("_A");
        Ensure(material.shaderKeywords.SequenceEqual(new[] { "_B" }) && !material.IsKeywordEnabled("_A"), "DisableKeyword");
        Ensure(!Shader.IsKeywordEnabled("_B"), "a material keyword is not a global keyword");
        var keyword = new LocalKeyword(shader, "_C");
        Ensure(keyword.name == "_C" && keyword.isValid && !keyword.isDynamic && !keyword.isOverridable && keyword.type == ShaderKeywordType.UserDefined && keyword.ToString() == "_C", "LocalKeyword");
        Ensure(keyword == new LocalKeyword(shader, "_C") && keyword != new LocalKeyword(shader, "_D") && keyword != new LocalKeyword(DisunityHost.RegisterShader("Test/ShaderMaterial/Keywords2"), "_C") && keyword.Equals((object)new LocalKeyword(shader, "_C"))
            && keyword.GetHashCode() == new LocalKeyword(shader, "_C").GetHashCode(), "LocalKeyword equality needs the same name and shader");
        Ensure(!default(LocalKeyword).isValid && default(LocalKeyword).type == ShaderKeywordType.None && default(LocalKeyword).name == "", "a default LocalKeyword is invalid");
        material.EnableKeyword(ref keyword);
        Ensure(material.IsKeywordEnabled(ref keyword) && material.IsKeywordEnabled("_C") && material.enabledKeywords.Any(known => known == keyword), "EnableKeyword(ref LocalKeyword)");
        material.DisableKeyword(ref keyword);
        Ensure(!material.IsKeywordEnabled(ref keyword), "DisableKeyword(ref LocalKeyword)");
        material.SetKeyword(ref keyword, true);
        Ensure(material.IsKeywordEnabled("_C"), "SetKeyword true");
        material.SetKeyword(ref keyword, false);
        Ensure(!material.IsKeywordEnabled("_C"), "SetKeyword false");
        material.shaderKeywords = ["_X", "_Y", "_X"];
        Ensure(material.shaderKeywords.SequenceEqual(new[] { "_X", "_Y" }), "setting shaderKeywords replaces the set");
        material.enabledKeywords = [new LocalKeyword(shader, "_Z")];
        Ensure(material.shaderKeywords.SequenceEqual(new[] { "_Z" }) && material.enabledKeywords.Length == 1 && material.enabledKeywords[0].name == "_Z", "setting enabledKeywords replaces the set");
        Throws<ArgumentNullException>(() => material.EnableKeyword((string)null!), "a null keyword");
        Throws<ArgumentNullException>(() => material.shaderKeywords = null!, "null keywords");
        Throws<ArgumentNullException>(() => new LocalKeyword(shader, null!), "a LocalKeyword without a name");
        Throws<ArgumentNullException>(() => new LocalKeyword(null!, "_A"), "a LocalKeyword without a shader");
    }

    private static void MaterialCopyAndLerp()
    {
        var shader = DisunityHost.RegisterShader("Test/ShaderMaterial/Copy");
        var source = new Material(shader) { name = "Source" };
        source.SetFloat("_Cutoff", 0.5f);
        source.SetColor("_Color", Color.red);
        source.SetTexture("_MainTex", new Texture2D(2, 2));
        source.SetFloatArray("_Weights", new[] { 1f, 2f });
        source.EnableKeyword("_ALPHATEST");
        source.renderQueue = 2450;
        source.SetOverrideTag("RenderType", "TransparentCutout");
        source.SetShaderPassEnabled("ShadowCaster", false);
        source.enableInstancing = true;
        var target = new Material(DisunityHost.RegisterShader("Test/ShaderMaterial/CopyTarget"));
        target.SetFloat("_Gone", 1f);
        target.CopyPropertiesFromMaterial(source);
        Ensure(ReferenceEquals(target.shader, shader) && target.GetFloat("_Cutoff") == 0.5f && target.GetColor("_Color") == Color.red && ReferenceEquals(target.GetTexture("_MainTex"), source.GetTexture("_MainTex")), "CopyPropertiesFromMaterial copies shader and values");
        Ensure(!target.HasProperty("_Gone") && target.GetFloatArray("_Weights")!.Length == 2 && target.IsKeywordEnabled("_ALPHATEST") && target.renderQueue == 2450 && target.enableInstancing, "it replaces the old values and copies keywords, queue and flags");
        Ensure(target.GetTag("RenderType", false) == "TransparentCutout" && !target.GetShaderPassEnabled("ShadowCaster"), "it copies tags and disabled passes");
        source.SetFloat("_Cutoff", 0.9f);
        source.GetFloatArray("_Weights")![0] = 100f;
        Ensure(target.GetFloat("_Cutoff") == 0.5f && target.GetFloatArray("_Weights")![0] == 1f, "the copy is independent");
        var duplicate = new Material(source);
        Ensure(duplicate.name == "Source" && ReferenceEquals(duplicate.shader, shader) && duplicate.GetFloat("_Cutoff") == 0.9f && duplicate.IsKeywordEnabled("_ALPHATEST"), "Material(Material) copies everything");
        Throws<ArgumentNullException>(() => target.CopyPropertiesFromMaterial(null!), "copying from null");
        NotSupported(() => target.CopyMatchingPropertiesFromMaterial(source), "CopyMatchingPropertiesFromMaterial", "Material.CopyMatchingPropertiesFromMaterial");

        var start = new Material(shader);
        var end = new Material(shader);
        start.SetFloat("_Value", 0f);
        end.SetFloat("_Value", 10f);
        start.SetColor("_Tint", Color.black);
        end.SetColor("_Tint", Color.white);
        start.SetFloat("_OnlyStart", 1f);
        end.SetFloat("_OnlyEnd", 2f);
        var result = new Material(shader);
        result.SetFloat("_Value", -5f);
        result.Lerp(start, end, 0.25f);
        Ensure(MathF.Abs(result.GetFloat("_Value") - 2.5f) < 1e-6f && MathF.Abs(result.GetColor("_Tint").r - 0.25f) < 1e-6f, "Lerp interpolates floats and colors");
        Ensure(!result.HasProperty("_OnlyStart") && !result.HasProperty("_OnlyEnd"), "Lerp leaves properties only one material has");
        result.Lerp(start, end, 2f);
        Ensure(result.GetFloat("_Value") == 10f && result.GetColor("_Tint") == Color.white, "Lerp keeps t within 0 and 1");
        result.Lerp(start, end, -1f);
        Ensure(result.GetFloat("_Value") == 0f, "Lerp below 0 is the start");
        Throws<ArgumentNullException>(() => result.Lerp(null!, end, 0.5f), "Lerp from null");
    }

    private static void MaterialMisc()
    {
        var material = new Material(DisunityHost.RegisterShader("Test/ShaderMaterial/Misc"));
        Ensure(material.GetTag("RenderType", false, "fallback") == "fallback" && material.GetTag("RenderType", true) == "", "an unknown tag returns the default");
        material.SetOverrideTag("RenderType", "Opaque");
        Ensure(material.GetTag("RenderType", false, "fallback") == "Opaque" && material.GetTag("RenderType", false) == "Opaque", "an override tag");
        material.SetOverrideTag("RenderType", "");
        Ensure(material.GetTag("RenderType", false, "fallback") == "fallback", "an empty override removes the tag");
        Ensure(material.GetShaderPassEnabled("ShadowCaster"), "passes start enabled");
        material.SetShaderPassEnabled("ShadowCaster", false);
        Ensure(!material.GetShaderPassEnabled("ShadowCaster") && material.GetShaderPassEnabled("Other"), "a pass can be disabled");
        material.SetShaderPassEnabled("ShadowCaster", true);
        Ensure(material.GetShaderPassEnabled("ShadowCaster"), "and enabled again");
        NotSupported(() => material.SetPass(0), "SetPass", "Material.SetPass");
        NotSupported(() => material.FindPass("Forward"), "FindPass", "Material.FindPass");
        NotSupported(() => material.GetPassName(0), "GetPassName", "Material.GetPassName");
        NotSupported(() => material.ComputeCRC(), "ComputeCRC", "Material.ComputeCRC");
        var obsolete = typeof(Material);
        NotSupported(() => Invoke(obsolete.GetConstructor([typeof(string)])!, "Shader \"X\" {}"), "Material(string)", "Material(string)");
        NotSupported(() => Invoke(obsolete.GetMethod("Create", [typeof(string)])!, "Shader \"X\" {}"), "Material.Create", "Material.Create");
    }

    /// <summary>Calls an obsolete member through reflection, so the test needs no warning suppression.</summary>
    private static void Invoke(System.Reflection.MethodBase member, string argument)
    {
        try { _ = member is System.Reflection.ConstructorInfo constructor ? constructor.Invoke([argument]) : member.Invoke(null, [argument]); }
        catch (System.Reflection.TargetInvocationException error) { throw error.InnerException!; }
    }

    private static void PropertyBlocks()
    {
        var block = new MaterialPropertyBlock();
        Ensure(block.isEmpty && !block.HasProperty("_Color"), "a new block is empty");
        block.SetColor("_Color", Color.red);
        Ensure(!block.isEmpty && block.GetColor("_Color") == Color.red && block.HasColor("_Color") && block.HasVector("_Color") && block.HasProperty(Shader.PropertyToID("_Color")), "a color in a block");
        block.SetFloat("_F", 1f);
        block.SetFloat(Shader.PropertyToID("_F2"), 2f);
        block.SetInt("_I", 3);
        block.SetInteger("_Int", 4);
        block.SetVector("_V", Vector4.one);
        block.SetMatrix("_M", Matrix4x4.identity);
        block.SetColor(Shader.PropertyToID("_C2"), Color.blue);
        block.SetVector(Shader.PropertyToID("_V2"), Vector4.zero);
        block.SetMatrix(Shader.PropertyToID("_M2"), Matrix4x4.zero);
        block.SetInt(Shader.PropertyToID("_I2"), 5);
        block.SetInteger(Shader.PropertyToID("_Int2"), 6);
        Ensure(block.GetFloat("_F") == 1f && block.GetFloat(Shader.PropertyToID("_F2")) == 2f && block.GetInt("_I") == 3 && block.GetInteger("_Int") == 4 && block.GetInt(Shader.PropertyToID("_I2")) == 5 && block.GetInteger(Shader.PropertyToID("_Int2")) == 6, "scalars");
        Ensure(block.GetVector("_V") == Vector4.one && block.GetMatrix("_M") == Matrix4x4.identity && block.GetColor(Shader.PropertyToID("_C2")) == Color.blue && block.GetVector(Shader.PropertyToID("_V2")) == Vector4.zero && block.GetMatrix(Shader.PropertyToID("_M2")) == Matrix4x4.zero, "vectors and matrices");
        Ensure(block.HasFloat("_F") && block.HasInt("_I") && block.HasInteger("_Int") && !block.HasInteger("_I") && block.HasMatrix("_M") && block.HasFloat(Shader.PropertyToID("_F2")) && block.HasInt(Shader.PropertyToID("_I2")) && block.HasInteger(Shader.PropertyToID("_Int2"))
            && block.HasMatrix(Shader.PropertyToID("_M2")) && block.HasVector(Shader.PropertyToID("_V2")) && block.HasColor(Shader.PropertyToID("_C2")), "Has methods");
        var texture = new Texture2D(2, 2);
        var target = new RenderTexture(8, 8, 0);
        block.SetTexture("_T", texture);
        block.SetTexture(Shader.PropertyToID("_T2"), target);
        block.SetTexture("_T3", target, RenderTextureSubElement.Depth);
        block.SetTexture(Shader.PropertyToID("_T4"), target, RenderTextureSubElement.Color);
        Ensure(ReferenceEquals(block.GetTexture("_T"), texture) && ReferenceEquals(block.GetTexture(Shader.PropertyToID("_T2")), target) && ReferenceEquals(block.GetTexture("_T3"), target) && ReferenceEquals(block.GetTexture(Shader.PropertyToID("_T4")), target)
            && block.HasTexture("_T") && block.HasTexture(Shader.PropertyToID("_T2")), "textures");
        block.SetFloatArray("_FA", new[] { 1f, 2f });
        block.SetFloatArray(Shader.PropertyToID("_FA2"), new List<float> { 1f });
        block.SetFloatArray("_FA3", new List<float> { 1f });
        block.SetFloatArray(Shader.PropertyToID("_FA4"), new[] { 1f });
        block.SetVectorArray("_VA", new[] { Vector4.one });
        block.SetVectorArray(Shader.PropertyToID("_VA2"), new List<Vector4> { Vector4.one });
        block.SetVectorArray("_VA3", new List<Vector4> { Vector4.one });
        block.SetVectorArray(Shader.PropertyToID("_VA4"), new[] { Vector4.one });
        block.SetMatrixArray("_MA", new[] { Matrix4x4.identity });
        block.SetMatrixArray(Shader.PropertyToID("_MA2"), new List<Matrix4x4> { Matrix4x4.identity });
        block.SetMatrixArray("_MA3", new List<Matrix4x4> { Matrix4x4.identity });
        block.SetMatrixArray(Shader.PropertyToID("_MA4"), new[] { Matrix4x4.identity });
        var (floats, vectors, matrices) = (new List<float>(), new List<Vector4>(), new List<Matrix4x4>());
        block.GetFloatArray("_FA", floats);
        block.GetFloatArray(Shader.PropertyToID("_FA"), floats);
        block.GetVectorArray("_VA", vectors);
        block.GetVectorArray(Shader.PropertyToID("_VA"), vectors);
        block.GetMatrixArray("_MA", matrices);
        block.GetMatrixArray(Shader.PropertyToID("_MA"), matrices);
        Ensure(block.GetFloatArray("_FA")!.Length == 2 && block.GetFloatArray(Shader.PropertyToID("_FA2"))!.Length == 1 && block.GetVectorArray("_VA")!.Length == 1 && block.GetVectorArray(Shader.PropertyToID("_VA2"))!.Length == 1
            && block.GetMatrixArray("_MA")!.Length == 1 && block.GetMatrixArray(Shader.PropertyToID("_MA2"))!.Length == 1 && floats.Count == 2 && vectors.Count == 1 && matrices.Count == 1 && block.GetFloatArray("_Missing") == null, "arrays");
    }

    private static void PropertyBlockBuffersAndClear()
    {
        var block = new MaterialPropertyBlock();
        var texture = new Texture2D(2, 2);
        block.SetTexture("_T", texture);
        block.SetBuffer("_B", (ComputeBuffer)null!);
        block.SetBuffer(Shader.PropertyToID("_B2"), (GraphicsBuffer)null!);
        block.SetBuffer("_B3", (GraphicsBuffer)null!);
        block.SetBuffer(Shader.PropertyToID("_B4"), (ComputeBuffer)null!);
        block.SetConstantBuffer("_CB", (ComputeBuffer)null!, 0, 16);
        block.SetConstantBuffer("_CB2", (GraphicsBuffer)null!, 0, 16);
        block.SetConstantBuffer(Shader.PropertyToID("_CB3"), (ComputeBuffer)null!, 0, 16);
        block.SetConstantBuffer(Shader.PropertyToID("_CB4"), (GraphicsBuffer)null!, 0, 16);
        Ensure(block.HasBuffer("_B") && block.HasBuffer(Shader.PropertyToID("_B2")) && block.HasConstantBuffer("_CB") && block.HasConstantBuffer(Shader.PropertyToID("_CB4")) && !block.HasBuffer("_CB"), "buffers");

        block.AddFloat("_Added", 1f);
        block.AddFloat(Shader.PropertyToID("_Added2"), 2f);
        block.AddVector("_AddedV", Vector4.one);
        block.AddVector(Shader.PropertyToID("_AddedV2"), Vector4.one);
        block.AddColor("_AddedC", Color.red);
        block.AddColor(Shader.PropertyToID("_AddedC2"), Color.red);
        block.AddMatrix("_AddedM", Matrix4x4.identity);
        block.AddMatrix(Shader.PropertyToID("_AddedM2"), Matrix4x4.identity);
        block.AddTexture("_AddedT", texture);
        block.AddTexture(Shader.PropertyToID("_AddedT2"), texture);
        Ensure(block.GetFloat("_Added") == 1f && block.GetFloat("_Added2") == 2f && block.GetColor("_AddedC") == Color.red && block.HasTexture("_AddedT2") && block.HasMatrix("_AddedM2") && block.HasVector("_AddedV2"), "the Add methods set values");

        block.Clear();
        Ensure(block.isEmpty && !block.HasProperty("_Color") && block.GetColor("_Color") == default && block.GetTexture("_T") == null, "Clear empties the block");
        block.SetFloat("_F", 1f);
        Ensure(!block.isEmpty, "a block can be filled again");

    }

    private static void PropertyBlockProbes()
    {
        var block = new MaterialPropertyBlock();
        SphericalHarmonicsL2[] probes = [default];
        var probeList = new List<SphericalHarmonicsL2>(probes);
        NotSupported(() => block.CopySHCoefficientArraysFrom(probeList), "CopySHCoefficientArraysFrom(List)", "MaterialPropertyBlock.CopySHCoefficientArraysFrom");
        NotSupported(() => block.CopySHCoefficientArraysFrom(probes), "CopySHCoefficientArraysFrom(array)", "MaterialPropertyBlock.CopySHCoefficientArraysFrom");
        NotSupported(() => block.CopySHCoefficientArraysFrom(probeList, 0, 0, 1), "CopySHCoefficientArraysFrom(List, range)", "MaterialPropertyBlock.CopySHCoefficientArraysFrom");
        NotSupported(() => block.CopySHCoefficientArraysFrom(probes, 0, 0, 1), "CopySHCoefficientArraysFrom(array, range)", "MaterialPropertyBlock.CopySHCoefficientArraysFrom");
        Vector4[] occlusion = [Vector4.one];
        var occlusionList = new List<Vector4>(occlusion);
        NotSupported(() => block.CopyProbeOcclusionArrayFrom(occlusionList), "CopyProbeOcclusionArrayFrom(List)", "MaterialPropertyBlock.CopyProbeOcclusionArrayFrom");
        NotSupported(() => block.CopyProbeOcclusionArrayFrom(occlusion), "CopyProbeOcclusionArrayFrom(array)", "MaterialPropertyBlock.CopyProbeOcclusionArrayFrom");
        NotSupported(() => block.CopyProbeOcclusionArrayFrom(occlusionList, 0, 0, 1), "CopyProbeOcclusionArrayFrom(List, range)", "MaterialPropertyBlock.CopyProbeOcclusionArrayFrom");
        NotSupported(() => block.CopyProbeOcclusionArrayFrom(occlusion, 0, 0, 1), "CopyProbeOcclusionArrayFrom(array, range)", "MaterialPropertyBlock.CopyProbeOcclusionArrayFrom");

        var coefficients = new SphericalHarmonicsL2();
        coefficients[1, 3] = 2f;
        var same = coefficients;
        Ensure(coefficients[1, 3] == 2f && coefficients[0, 3] == 0f && (coefficients * 2f)[1, 3] == 4f && (2f * coefficients)[1, 3] == 4f && (coefficients + coefficients)[1, 3] == 4f, "SphericalHarmonicsL2 holds and combines coefficients");
        Ensure(coefficients == same && coefficients != default && coefficients.Equals((object)coefficients) && coefficients.GetHashCode() == coefficients.GetHashCode(), "SphericalHarmonicsL2 equality");
        coefficients.Clear();
        Ensure(coefficients == default, "Clear");
        Throws<IndexOutOfRangeException>(() => _ = coefficients[3, 0], "an out-of-range channel");
        NotSupported(() => coefficients.AddAmbientLight(Color.white), "AddAmbientLight", "SphericalHarmonicsL2.AddAmbientLight");
    }

    private static void SpriteMaterials()
    {
        Ensure(Material.SpriteUnlit is Material && Material.SpriteLit is UnityEngine.Object && Material.SpriteUnlit != Material.SpriteLit, "the sprite materials are Materials and Objects");
        Ensure(Material.SpriteUnlit.SpriteMaterial == BuiltinSpriteMaterial.Unlit && Material.SpriteLit.SpriteMaterial == BuiltinSpriteMaterial.Lit, "they keep their native identity");
        Ensure(Material.SpriteUnlit.name == "Sprites-Default" && (bool)Material.SpriteLit.shader && Material.SpriteLit.shader != Material.SpriteUnlit.shader, "they have names and shaders");
        Ensure(new Material(BuiltinSpriteMaterial.Lit).SpriteMaterial == BuiltinSpriteMaterial.Lit && new Material(Material.SpriteLit).SpriteMaterial == BuiltinSpriteMaterial.Lit, "a sprite material can be built and copied");
        Ensure(ReferenceEquals(Shader.Find("Sprites/Default"), Material.SpriteUnlit.shader) && ReferenceEquals(Shader.Find("Universal Render Pipeline/2D/Sprite-Lit-Default"), Material.SpriteLit.shader),
            "the sprite shaders are registered, so Shader.Find returns the material's shader");
        Ensure(Material.SpriteUnlit.color == Color.white && Material.SpriteLit.color == Color.white && Material.SpriteUnlit.GetColor("_Color") == Color.white, "sprite materials start white");
        Ensure(Material.SpriteUnlit.shader.renderQueue == 3000 && Material.SpriteLit.shader.renderQueue == 3000 && Material.SpriteUnlit.shader.passCount == 1, "sprite shaders report Unity's queue");
        NotSupported(() => _ = Material.SpriteLit.shader.passCount, "the URP sprite shader's passes are unknown", "Shader.passCount");
        Throws<InvalidOperationException>(() => UnityEngine.Object.Destroy(Material.SpriteLit), "destroying the lit sprite material", "SpriteLit");
        Throws<InvalidOperationException>(() => UnityEngine.Object.DestroyImmediate(Material.SpriteUnlit), "destroying the unlit sprite material", "SpriteUnlit");
        Ensure(Material.SpriteLit && Material.SpriteUnlit, "a refused destroy leaves the sprite materials alive");
        var copy = new Material(Material.SpriteLit);
        UnityEngine.Object.Destroy(copy);
        Ensure(copy == null && Material.SpriteLit, "a copy of a sprite material can be destroyed");
        Throws<NotSupportedException>(() => new Material((BuiltinSpriteMaterial)7), "an unknown sprite material", "7");
        var custom = new Material(DisunityHost.RegisterShader("Test/ShaderMaterial/Custom"));
        NotSupported(() => _ = custom.SpriteMaterial, "a custom material has no sprite identity", "Material.SpriteMaterial");

        DisunityHost.LoadWorld();
        try
        {
            var renderer = DisunityHost.CreateGameObject().AddComponent<SpriteRenderer>();
            renderer.sharedMaterial = Material.SpriteLit;
            Ensure(ReferenceEquals(renderer.sharedMaterial, Material.SpriteLit), "a renderer keeps a sprite material");
            renderer.sharedMaterial = Material.SpriteUnlit;
            Ensure(ReferenceEquals(renderer.sharedMaterial, Material.SpriteUnlit), "and changes it");
            renderer.sharedMaterial = custom;
            Ensure(ReferenceEquals(renderer.sharedMaterial, custom), "a renderer keeps a custom material");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
