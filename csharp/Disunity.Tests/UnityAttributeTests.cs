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
        FormerlySerializedAsKeepsEveryOldName();
        SupportedOnRenderPipelineKeepsItsTypes();
        ResourcePathAttributesExpandTheirPaths();
        MovedFromKnowsWhetherTheAssemblyChanged();
        PropertyAttributesShareTheirBaseClass();
        Console.WriteLine("PASS: Unity attribute classes keep their arguments and work as reflection reads them");
    }

    [RequireComponent(typeof(Rigidbody2D))]
    [RequireComponent(typeof(Camera), typeof(SpriteRenderer))]
    [RequireComponent(typeof(Transform), typeof(Collider2D), typeof(Rigidbody2D))]
    [AddComponentMenu("Game/Probe", 7)]
    [DisallowMultipleComponent, ExecuteAlways, DefaultExecutionOrder(-50), HelpURL("https://example.com/probe")]
    private sealed class Decorated : MonoBehaviour
    {
        [Header("Movement"), Range(0, 2), Tooltip("units per second"), HideInInspector]
        [FormerlySerializedAs("oldSpeed"), FormerlySerializedAs("ancientSpeed")]
        public float speed;

        [ContextMenu("Reset speed")]
        public void ResetSpeed() => speed = 0;

        [ContextMenu("Can reset", true, 12)]
        public bool CanReset() => true;
    }

    private static void RequireComponentKeepsEachRequiredType()
    {
        var required = typeof(Decorated).GetCustomAttributes<RequireComponent>().OrderBy(a => a.m_Type1 is null ? 0 : a.m_Type2 is null ? 1 : 2).ToArray();
        Expect.Equal(3, required.Length, "RequireComponent may be applied more than once");
        Expect.That(required[0].m_Type0 == typeof(Rigidbody2D) && required[0].m_Type1 is null && required[0].m_Type2 is null, "one required type");
        Expect.That(required[1].m_Type0 == typeof(Camera) && required[1].m_Type1 == typeof(SpriteRenderer) && required[1].m_Type2 is null, "two required types");
        Expect.That(required[2].m_Type0 == typeof(Transform) && required[2].m_Type1 == typeof(Collider2D) && required[2].m_Type2 == typeof(Rigidbody2D),
            "three required types");
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
        Expect.That(typeof(Decorated).GetCustomAttribute<HelpURLAttribute>()!.URL == "https://example.com/probe", "HelpURL");
        Expect.That(typeof(Decorated).GetField("speed")!.GetCustomAttribute<HideInInspector>() is not null, "HideInInspector on a field");
    }

    private static bool Has<T>() where T : Attribute => typeof(Decorated).GetCustomAttribute<T>() is not null;

    private static void FormerlySerializedAsKeepsEveryOldName()
    {
        var names = typeof(Decorated).GetField("speed")!.GetCustomAttributes<FormerlySerializedAsAttribute>().Select(a => a.oldName).Order().ToArray();
        Expect.Sequence(new[] { "ancientSpeed", "oldSpeed" }, names, "old field names");
    }

    private static void SupportedOnRenderPipelineKeepsItsTypes()
    {
        var one = new SupportedOnRenderPipelineAttribute(typeof(Decorated));
        Expect.Sequence(new[] { typeof(Decorated) }, one.renderPipelineTypes, "one pipeline type");
        var two = new SupportedOnRenderPipelineAttribute(typeof(Decorated), typeof(RenderingLayerMask));
        Expect.Sequence(new[] { typeof(Decorated), typeof(RenderingLayerMask) }, two.renderPipelineTypes, "several pipeline types");
        Expect.That(!one.isSupportedOnCurrentPipeline, "a type that names a pipeline is not supported when no pipeline asset is active");
        Expect.That(new SupportedOnRenderPipelineAttribute().isSupportedOnCurrentPipeline, "an attribute that names no pipeline is supported everywhere");
    }

    private static void ResourcePathAttributesExpandTheirPaths()
    {
        var formatted = new ResourceFormattedPathsAttribute("Assets/Luts/lut_{0}.png", 2, 5);
        Expect.Sequence(new[] { "Assets/Luts/lut_2.png", "Assets/Luts/lut_3.png", "Assets/Luts/lut_4.png" }, formatted.paths, "formatted paths exclude the end");
        Expect.That(!formatted.isField && formatted.location == SearchType.ProjectPath, "formatted paths belong to a type, in the project");
        Expect.Equal(0, new ResourceFormattedPathsAttribute("x{0}", 3, 3).paths.Length, "an empty range gives no paths");
        Expect.Equal(0, new ResourceFormattedPathsAttribute("x{0}", 5, 3).paths.Length, "a reversed range gives no paths");
        Expect.Sequence(new[] { "Lut_0", "Lut_1" }, new ResourceFormattedPathsAttribute("Lut_{0}", 0, 2, SearchType.ShaderName).paths, "a shader name format");
        Expect.Equal(SearchType.ShaderName, new ResourceFormattedPathsAttribute("Lut_{0}", 0, 2, SearchType.ShaderName).location, "location");
        var single = new ResourcePathAttribute("Hidden/Blit", SearchType.ShaderName);
        Expect.That(single.paths.SequenceEqual(["Hidden/Blit"]) && single.isField && single.location == SearchType.ShaderName, "ResourcePath names one field's path");
        var many = new ResourcePathsAttribute(["a", "b"]);
        Expect.That(many.paths.SequenceEqual(["a", "b"]) && !many.isField && many.location == SearchType.ProjectPath, "ResourcePaths keeps its array");
    }

    private static void MovedFromKnowsWhetherTheAssemblyChanged()
    {
        Expect.That(new MovedFromAttribute(true, "Old.Namespace", "Old.Assembly", null).IsInDifferentAssembly, "a source assembly means a different assembly");
        Expect.That(!new MovedFromAttribute(true, "Old.Namespace", null, "OldName").IsInDifferentAssembly, "no source assembly means the same assembly");
        Expect.That(!new MovedFromAttribute("Old.Namespace").IsInDifferentAssembly, "a source namespace alone is the same assembly");
    }

    private static void PropertyAttributesShareTheirBaseClass()
    {
        var field = typeof(Decorated).GetField("speed")!;
        var attributes = field.GetCustomAttributes<PropertyAttribute>().ToArray();
        Expect.Equal(3, attributes.Length, "Header, Range and Tooltip are found as PropertyAttribute");
        Expect.That(attributes.OfType<HeaderAttribute>().Single().header == "Movement", "Header text");
        var range = attributes.OfType<RangeAttribute>().Single();
        Expect.That(range.min == 0 && range.max == 2, "Range bounds");
        Expect.That(attributes.OfType<TooltipAttribute>().Single().tooltip == "units per second", "Tooltip text");
    }
}
