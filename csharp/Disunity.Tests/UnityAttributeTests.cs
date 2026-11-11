using System.Reflection;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Scripting.APIUpdating;
using UnityEngine.Serialization;

/// <summary>
/// Unity's attribute classes as a player's assemblies use them: applied to game types and read
/// back through reflection, with the arguments their constructors were given.
/// </summary>
internal static class UnityAttributeTests
{
    public static void Run()
    {
        RequireComponentKeepsEachRequiredType();
        MenuAttributesKeepTheirArguments();
        AssetMenuAndStartUpAttributesKeepTheirArguments();
        FormerlySerializedAsKeepsEveryOldName();
        MovedFromKeepsWhereTheTypeLived();
        IsTypeSupportedOnRenderPipelineMatchesTheNamedPipelines();
        ResourcePathAttributesKeepTheirPaths();
        PropertyAttributesShareTheirBaseClass();
        NativeCallbackMarkerKeepsItsDelegateType();
        Console.WriteLine("PASS: Unity attribute classes keep their arguments and work as reflection reads them");
    }

    [RequireComponent(typeof(Rigidbody2D))]
    [RequireComponent(typeof(Camera), typeof(SpriteRenderer))]
    [RequireComponent(typeof(Transform), typeof(Collider2D), typeof(Rigidbody2D))]
    [AddComponentMenu("Game/Probe", 7)]
    [DisallowMultipleComponent, ExecuteAlways, SelectionBase, DefaultExecutionOrder(-50)]
    private sealed class Decorated : MonoBehaviour
    {
        [Header("Movement"), Range(0, 2), Tooltip("units per second"), Space(12), ColorUsage(false, true), Delayed, TextArea(2, 6), HideInInspector]
        [FormerlySerializedAs("oldSpeed"), FormerlySerializedAs("ancientSpeed")]
        public float speed;

        [ContextMenu("Reset speed")]
        public void ResetSpeed() => speed = 0;

        [ContextMenu("Can reset", true, 12)]
        public bool CanReset() => true;
    }

    [HelpURL("https://example.com/probe")]
    private sealed class DecoratedWithHelp : MonoBehaviour { }

    private static void RequireComponentKeepsEachRequiredType()
    {
        var required = typeof(Decorated).GetCustomAttributes<RequireComponent>().OrderBy(a => a.m_Type1 is null ? 0 : a.m_Type2 is null ? 1 : 2).ToArray();
        Expect.Equal(3, required.Length, "RequireComponent may be applied more than once");
        Expect.That(required[0].m_Type0 == typeof(Rigidbody2D) && required[0].m_Type1 is null && required[0].m_Type2 is null, "one required type");
        Expect.That(required[1].m_Type0 == typeof(Camera) && required[1].m_Type1 == typeof(SpriteRenderer) && required[1].m_Type2 is null, "two required types");
        Expect.That(required[2].m_Type0 == typeof(Transform) && required[2].m_Type1 == typeof(Collider2D) && required[2].m_Type2 == typeof(Rigidbody2D),
            "three required types");
    }

    [CreateAssetMenu(fileName = "Probe", menuName = "Game/Probe asset", order = 4)]
    private sealed class ProbeAsset : ScriptableObject;

    [RuntimeInitializeOnLoadMethod]
    private static void AfterScene() { }

    [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.SubsystemRegistration)]
    private static void Earliest() { }

    /// <summary>The asset menu keeps its named arguments; a start-up method runs after the scene loads unless it names
    /// a phase, and the phases carry the numbers a player's RuntimeInitializeOnLoads.json stores.</summary>
    private static void AssetMenuAndStartUpAttributesKeepTheirArguments()
    {
        var asset = typeof(ProbeAsset).GetCustomAttribute<CreateAssetMenuAttribute>()!;
        Expect.That(asset is { fileName: "Probe", menuName: "Game/Probe asset", order: 4 }, "CreateAssetMenu file name, menu name and order");
        Expect.That(new CreateAssetMenuAttribute() is { fileName: "", menuName: "", order: 0 }, "CreateAssetMenu starts empty");
        RuntimeInitializeLoadType Phase(string name) => typeof(UnityAttributeTests).GetMethod(name, BindingFlags.NonPublic | BindingFlags.Static)!
            .GetCustomAttribute<RuntimeInitializeOnLoadMethodAttribute>()!.loadType;
        Expect.That(Phase(nameof(AfterScene)) == RuntimeInitializeLoadType.AfterSceneLoad, "a start-up method runs after the scene loads by default");
        Expect.That(Phase(nameof(Earliest)) == RuntimeInitializeLoadType.SubsystemRegistration, "a start-up method keeps the phase it names");
        int[] numbers = [(int)RuntimeInitializeLoadType.AfterSceneLoad, (int)RuntimeInitializeLoadType.BeforeSceneLoad,
            (int)RuntimeInitializeLoadType.AfterAssembliesLoaded, (int)RuntimeInitializeLoadType.BeforeSplashScreen,
            (int)RuntimeInitializeLoadType.SubsystemRegistration];
        Expect.That(numbers.SequenceEqual([0, 1, 2, 3, 4]), "the phases carry the numbers players store");
        Expect.That(typeof(RuntimeInitializeOnLoadMethodAttribute).IsSubclassOf(typeof(UnityEngine.Scripting.PreserveAttribute)), "a start-up method is kept by the linker");
    }

    private static void MenuAttributesKeepTheirArguments()
    {
        var menu = typeof(Decorated).GetCustomAttribute<AddComponentMenu>()!;
        Expect.That(menu.componentMenu == "Game/Probe" && menu.componentOrder == 7, "AddComponentMenu name and order");
        var plain = new AddComponentMenu("Only/Name");
        Expect.That(plain.componentMenu == "Only/Name" && plain.componentOrder == 0, "AddComponentMenu order defaults to 0");
        var items = typeof(Decorated).GetMethods().Select(m => (m.Name, Attribute: m.GetCustomAttribute<ContextMenu>())).Where(m => m.Attribute is not null)
            .ToDictionary(m => m.Name, m => m.Attribute!);
        Expect.That(items["ResetSpeed"] is { menuItem: "Reset speed", validate: false, priority: 1000000 }, "ContextMenu defaults to a command at priority 1000000");
        Expect.That(items["CanReset"] is { menuItem: "Can reset", validate: true, priority: 12 }, "ContextMenu validate function and priority");
        Expect.That(Has<DisallowMultipleComponent>() && Has<ExecuteAlways>(), "marker attributes are applied");
        Expect.That(typeof(Decorated).GetCustomAttribute<DefaultExecutionOrder>()!.order == -50, "DefaultExecutionOrder");
        Expect.That(typeof(Decorated).GetField("speed")!.GetCustomAttribute<HideInInspector>() is not null, "HideInInspector on a field");
        Expect.That(typeof(DecoratedWithHelp).GetCustomAttribute<HelpURLAttribute>()!.URL == "https://example.com/probe" && Has<SelectionBaseAttribute>(), "HelpURL and SelectionBase");
    }

    private static bool Has<T>() where T : Attribute => typeof(Decorated).GetCustomAttribute<T>() is not null;

    private static void FormerlySerializedAsKeepsEveryOldName()
    {
        var names = typeof(Decorated).GetField("speed")!.GetCustomAttributes<FormerlySerializedAsAttribute>().Select(a => a.oldName).Order().ToArray();
        Expect.Sequence(new[] { "ancientSpeed", "oldSpeed" }, names, "old field names");
    }

    private class PipelineA { }
    private sealed class DerivedPipelineA : PipelineA { }
    private sealed class PipelineB { }
    private sealed class PipelineC { }

    [SupportedOnRenderPipeline(typeof(PipelineA))]
    private class OnlyOnA { }

    [SupportedOnRenderPipeline(typeof(PipelineA), typeof(PipelineB))]
    private sealed class OnAOrB { }

    [SupportedOnRenderPipeline]
    private sealed class OnNone { }

    private sealed class DerivedFromOnlyOnA : OnlyOnA { }

    private sealed class Unmarked { }

    private static void IsTypeSupportedOnRenderPipelineMatchesTheNamedPipelines()
    {
        Expect.That(SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(Unmarked), typeof(PipelineC)), "a type with no attribute is supported on every pipeline");
        Expect.That(SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(OnlyOnA), typeof(PipelineA)), "a type is supported on the pipeline its attribute names");
        Expect.That(!SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(OnlyOnA), typeof(PipelineB)), "a type is not supported on a pipeline its attribute does not name");
        Expect.That(SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(OnAOrB), typeof(PipelineB)), "any of several named pipelines supports the type");
        Expect.That(!SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(OnAOrB), typeof(PipelineC)), "an unnamed pipeline does not");
        Expect.That(SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(OnlyOnA), typeof(DerivedPipelineA)), "a pipeline derived from a named one supports the type (the engine's choice)");
        Expect.That(!SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(OnlyOnA), typeof(object)), "a base of a named pipeline does not");
        Expect.That(!SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(OnNone), typeof(PipelineA)), "an attribute that names no pipeline supports none");
        Expect.That(SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(DerivedFromOnlyOnA), typeof(PipelineA))
            && !SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(DerivedFromOnlyOnA), typeof(PipelineC)), "a derived type keeps the attribute of its base");
        Expect.Throws<ArgumentNullException>(() => SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(null!, typeof(PipelineA)), "a null type");
        Expect.Throws<ArgumentNullException>(() => SupportedOnRenderPipelineAttribute.IsTypeSupportedOnRenderPipeline(typeof(OnlyOnA), null!), "a null pipeline");
    }

    private static void ResourcePathAttributesKeepTheirPaths()
    {
        var formatted = new ResourceFormattedPathsAttribute("Assets/Luts/lut_{0}.png", 2, 5);
        Expect.Sequence(new[] { "Assets/Luts/lut_2.png", "Assets/Luts/lut_3.png", "Assets/Luts/lut_4.png" }, formatted.paths, "formatted paths exclude the end");
        Expect.Equal(0, new ResourceFormattedPathsAttribute("x{0}", 5, 3).paths.Length, "a reversed range gives no paths");
        var shader = new ResourceFormattedPathsAttribute("Lut_{0}", 0, 2, ResourcePathAttribute.SearchType.ShaderName);
        Expect.That(shader.location == ResourcePathAttribute.SearchType.ShaderName && formatted.location == ResourcePathAttribute.SearchType.ProjectPath, "location");
        var single = new ResourcePathAttribute("Hidden/Blit", ResourcePathAttribute.SearchType.ShaderName);
        Expect.That(single.path == "Hidden/Blit" && single.location == ResourcePathAttribute.SearchType.ShaderName, "ResourcePath names one path");
        var many = new ResourcePathsAttribute(["a", "b"]);
        Expect.That(many.paths.SequenceEqual(["a", "b"]) && many.location == ResourcePathAttribute.SearchType.ProjectPath, "ResourcePaths keeps its array");
    }

    private static void MovedFromKeepsWhereTheTypeLived()
    {
        var full = new MovedFromAttribute(false, "Old.Namespace", "Old.Assembly", "OldName");
        Expect.That(!full.AutoUpdateAPI && full.Namespace == "Old.Namespace" && full.Assembly == "Old.Assembly" && full.ClassName == "OldName", "the four-argument form keeps every part");
        var namespaceOnly = new MovedFromAttribute("Old.Namespace");
        Expect.That(namespaceOnly.AutoUpdateAPI && namespaceOnly.Namespace == "Old.Namespace" && namespaceOnly.Assembly is null && namespaceOnly.ClassName is null,
            "a source namespace alone updates the API and names no assembly or class");
    }

    private static void PropertyAttributesShareTheirBaseClass()
    {
        var field = typeof(Decorated).GetField("speed")!;
        var attributes = field.GetCustomAttributes<PropertyAttribute>().ToArray();
        Expect.Equal(7, attributes.Length, "Header, Range, Tooltip, Space, ColorUsage, Delayed and TextArea are found as PropertyAttribute");
        Expect.That(attributes.OfType<HeaderAttribute>().Single().header == "Movement", "Header text");
        var range = attributes.OfType<RangeAttribute>().Single();
        Expect.That(range.min == 0 && range.max == 2, "Range bounds");
        Expect.That(attributes.OfType<TooltipAttribute>().Single().tooltip == "units per second", "Tooltip text");
        Expect.That(attributes.OfType<SpaceAttribute>().Single().height == 12 && new SpaceAttribute().height == 8, "Space height and its default");
        var lines = attributes.OfType<TextAreaAttribute>().Single();
        Expect.That(lines.minLines == 2 && lines.maxLines == 6 && new TextAreaAttribute().minLines == 3 && new TextAreaAttribute().maxLines == 3, "TextArea line counts and defaults");
        Expect.That(attributes.OfType<DelayedAttribute>().Count() == 1, "Delayed is a property attribute");
        var color = attributes.OfType<ColorUsageAttribute>().Single();
        Expect.That(!color.showAlpha && color.hdr && new ColorUsageAttribute(true).showAlpha && !new ColorUsageAttribute(true).hdr, "ColorUsage alpha and HDR flags");
    }

    private delegate void Callback(int value);

    [AOT.MonoPInvokeCallback(typeof(Callback))]
    private static void Called(int value) { }

    /// <summary>Transport reads the marker on its native callbacks; a module facade forwards it once the engine defines it.</summary>
    private static void NativeCallbackMarkerKeepsItsDelegateType()
    {
        var marker = typeof(UnityAttributeTests).GetMethod(nameof(Called), BindingFlags.NonPublic | BindingFlags.Static)!
            .GetCustomAttribute<AOT.MonoPInvokeCallbackAttribute>();
        Expect.That(marker?.type == typeof(Callback), "MonoPInvokeCallback keeps the delegate type it names");
        Expect.That(Disunity.Hosting.ModuleFacades.Forwards(typeof(Disunity.Hosting.DisunityHost).Assembly, "AOT.MonoPInvokeCallbackAttribute"),
            "a module facade forwards AOT.MonoPInvokeCallbackAttribute");
    }
}
