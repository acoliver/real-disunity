using Disunity.Hosting;
using Unity.Collections;
using UnityEngine;
using UnityEngine.Networking;
using UnityEngine.Rendering;
using UnityEngine.U2D;
using UnityEngine.UIElements;

/// <summary>Engine types that render pipeline and UI packages mention, so every assembly of a player loads its types.</summary>
internal static class RenderTypeTests
{
    private static readonly System.Reflection.Assembly Engine = typeof(DisunityHost).Assembly;

    public static void Run()
    {
        FacadesForward();
        Console.WriteLine("PASS: the module facades forward the render type names a player's assemblies reference");
        GpuDrivenDataHoldsValues();
        Console.WriteLine("PASS: the GPU driven data structs load and hold their values");
        Light2DBaseIsAScript();
        Console.WriteLine("PASS: Light2DBase is an abstract script a package light derives from");
        UiInterfacesLoad();
        Console.WriteLine("PASS: IPointerEvent and IRuntimePanelComponent are public interfaces");
        LightProbesQueryRefusesQueries();
        Console.WriteLine("PASS: LightProbesQuery constructs and its queries name themselves as unsupported");
        ReadbackRequestRefusesMembers();
        Console.WriteLine("PASS: AsyncGPUReadbackRequest members name themselves as unsupported");
        ScriptHandlerIsAHandler();
        Console.WriteLine("PASS: DownloadHandlerScript derives from DownloadHandler and its callbacks name themselves as unsupported");
    }

    private static void FacadesForward()
    {
        foreach (var name in new[]
        {
            "UnityEngine.LightProbesQuery", "UnityEngine.Rendering.AsyncGPUReadbackRequest", "UnityEngine.Rendering.GPUDrivenLODGroupData",
            "UnityEngine.Rendering.GPUDrivenMeshLodInfo", "UnityEngine.Rendering.GPUDrivenPackedMaterialData",
            "UnityEngine.Rendering.GPUDrivenRendererGroupData", "UnityEngine.Rendering.GPUDrivenRendererMeshLodData",
            "UnityEngine.U2D.Light2DBase", "UnityEngine.UIElements.IPointerEvent", "UnityEngine.UIElements.IRuntimePanelComponent",
            "UnityEngine.Networking.DownloadHandlerScript",
        })
            Expect.That(ModuleFacades.Forwards(Engine, name), $"the module facades forward {name}");
    }

    private static void GpuDrivenDataHoldsValues()
    {
        var lod = new GPUDrivenMeshLodInfo { lodCount = 3, lodSlicingEnabled = true };
        Expect.That(lod.lodCount == 3 && lod.lodSlicingEnabled, "mesh LOD info");
        Expect.That(!default(GPUDrivenMeshLodInfo).lodSlicingEnabled && default(GPUDrivenMeshLodInfo).lodCount == 0, "mesh LOD info defaults");
        var material = new GPUDrivenPackedMaterialData { isTransparent = true, isMotionVectorsPassEnabled = false, isIndirectSupported = true };
        Expect.That(material.isTransparent && !material.isMotionVectorsPassEnabled && material.isIndirectSupported, "packed material data");
        var ids = new NativeArray<int>(2, Allocator.Temp);
        ids[0] = 7;
        ids[1] = 9;
        var group = new GPUDrivenLODGroupData { lodGroupID = ids, lodOffset = ids, lodCount = ids };
        Expect.That(group.lodGroupID.Length == 2 && group.lodOffset[1] == 9 && group.lodCount[0] == 7, "LOD group data");
        var renderers = new GPUDrivenRendererGroupData { rendererGroupID = ids };
        Expect.That(renderers.rendererGroupID[1] == 9 && !renderers.localToWorldMatrix.IsCreated, "renderer group data");
        var infos = new NativeArray<GPUDrivenMeshLodInfo>(1, Allocator.Temp);
        infos[0] = lod;
        var meshLods = new GPUDrivenRendererMeshLodData { rendererGroupID = ids, meshLodInfo = infos };
        Expect.That(meshLods.meshLodInfo[0].lodCount == 3 && meshLods.rendererGroupID[0] == 7, "renderer mesh LOD data");
        ids.Dispose();
        infos.Dispose();
    }

    private sealed class PackageLight : Light2DBase { }

    private static void Light2DBaseIsAScript()
    {
        Expect.That(typeof(Light2DBase).IsAbstract && typeof(Light2DBase).IsPublic, "Light2DBase is public and abstract");
        Expect.That(typeof(Light2DBase).BaseType == typeof(MonoBehaviour), "and a MonoBehaviour");
        Expect.That(typeof(PackageLight).BaseType == typeof(Light2DBase), "a class in another assembly can derive from it");
    }

    private static void UiInterfacesLoad()
    {
        Expect.That(typeof(IPointerEvent).IsInterface && typeof(IPointerEvent).IsPublic, "IPointerEvent");
        Expect.That(typeof(IRuntimePanelComponent).IsInterface && typeof(IRuntimePanelComponent).IsPublic, "IRuntimePanelComponent");
    }

    private static void RequireNamed(Action call, string member)
    {
        var error = Expect.Catch<NotSupportedException>(call, member);
        Expect.That(error.Message.Contains(member), $"{member} is named: {error.Message}");
    }

    private static void LightProbesQueryRefusesQueries()
    {
        var query = new LightProbesQuery(Allocator.Temp);
        query.Dispose();
        var positions = new NativeArray<Vector3>(1, Allocator.Temp);
        var coefficients = new NativeArray<SphericalHarmonicsL2>(1, Allocator.Temp);
        var occlusion = new NativeArray<Vector4>(1, Allocator.Temp);
        try
        {
            RequireNamed(() => query.CalculateInterpolatedLightAndOcclusionProbes(positions, coefficients, occlusion),
                "LightProbesQuery.CalculateInterpolatedLightAndOcclusionProbes");
            RequireNamed(() => query.CalculateInterpolatedLightAndOcclusionProbe(Vector3.zero, out _, out _),
                "LightProbesQuery.CalculateInterpolatedLightAndOcclusionProbe");
        }
        finally
        {
            positions.Dispose();
            coefficients.Dispose();
            occlusion.Dispose();
        }
    }

    private static void ReadbackRequestRefusesMembers()
    {
        var request = default(AsyncGPUReadbackRequest);
        RequireNamed(() => _ = request.done, "AsyncGPUReadbackRequest.done");
        RequireNamed(() => _ = request.hasError, "AsyncGPUReadbackRequest.hasError");
        RequireNamed(() => _ = request.layerCount, "AsyncGPUReadbackRequest.layerCount");
        RequireNamed(() => _ = request.layerDataSize, "AsyncGPUReadbackRequest.layerDataSize");
        RequireNamed(() => _ = request.width, "AsyncGPUReadbackRequest.width");
        RequireNamed(() => _ = request.height, "AsyncGPUReadbackRequest.height");
        RequireNamed(() => _ = request.depth, "AsyncGPUReadbackRequest.depth");
        RequireNamed(() => request.Update(), "AsyncGPUReadbackRequest.Update");
        RequireNamed(() => request.WaitForCompletion(), "AsyncGPUReadbackRequest.WaitForCompletion");
        RequireNamed(() => request.GetData<int>(), "AsyncGPUReadbackRequest.GetData");
    }

    private sealed class ScriptHandler : DownloadHandlerScript
    {
        public ScriptHandler() { }
        public ScriptHandler(byte[] buffer) : base(buffer) { }
        public bool CallReceiveData() => base.ReceiveData(new byte[2], 2);
        public void CallReceiveContentLength() => base.ReceiveContentLengthHeader(10);
    }

    private static void ScriptHandlerIsAHandler()
    {
        Expect.That(typeof(DownloadHandlerScript).BaseType == typeof(DownloadHandler), "a download handler");
        using var plain = new ScriptHandler();
        using var buffered = new ScriptHandler(new byte[16]);
        Expect.That(plain.data == null && buffered.text == null, "no content arrives");
        Expect.Throws<ArgumentNullException>(() => _ = new ScriptHandler(null!), "a null preallocated buffer");
        RequireNamed(() => plain.CallReceiveData(), "DownloadHandlerScript.ReceiveData");
        RequireNamed(() => plain.CallReceiveContentLength(), "DownloadHandlerScript.ReceiveContentLengthHeader");
    }
}
