// Unity's attribute classes, generated from a Unity 6000.3 player's engine module metadata: their
// base classes, usage, constructor signatures and public fields and properties. They carry
// metadata only; the engine reads none of them yet.

namespace Unity.Profiling
{
    [System.AttributeUsage((System.AttributeTargets)76, AllowMultiple = false)]
    public sealed class IgnoredByDeepProfilerAttribute : System.Attribute
    {
        public IgnoredByDeepProfilerAttribute() { }
    }
}

namespace UnityEngine
{
    public sealed class ImageEffectTransformsToLDR : System.Attribute
    {
        public ImageEffectTransformsToLDR() { }
    }

    public sealed class ImageEffectAllowedInSceneView : System.Attribute
    {
        public ImageEffectAllowedInSceneView() { }
    }

    public sealed class ImageEffectOpaque : System.Attribute
    {
        public ImageEffectOpaque() { }
    }

    [System.AttributeUsage((System.AttributeTargets)64)]
    public sealed class ImageEffectUsesCommandBuffer : System.Attribute
    {
        public ImageEffectUsesCommandBuffer() { }
    }

    [System.AttributeUsage((System.AttributeTargets)384, Inherited = true, AllowMultiple = false)]
    public abstract class PropertyAttribute : System.Attribute
    {
        public int order { get; set; }
        public bool applyToCollection { get; }
        protected PropertyAttribute() { }
        protected PropertyAttribute(bool applyToCollection) { this.applyToCollection = applyToCollection; }
    }

    [System.AttributeUsage((System.AttributeTargets)256, Inherited = true, AllowMultiple = false)]
    public class InspectorNameAttribute : PropertyAttribute
    {
        public readonly string displayName;
        public InspectorNameAttribute(string displayName) { this.displayName = displayName; }
    }

    [System.AttributeUsage((System.AttributeTargets)32767, Inherited = true, AllowMultiple = false)]
    public class TooltipAttribute : PropertyAttribute
    {
        public readonly string tooltip;
        public TooltipAttribute(string tooltip) { this.tooltip = tooltip; }
    }

    [System.AttributeUsage((System.AttributeTargets)256, Inherited = true, AllowMultiple = true)]
    public class SpaceAttribute : PropertyAttribute
    {
        public readonly float height;
        public SpaceAttribute() { }
        public SpaceAttribute(float height) { this.height = height; }
    }

    [System.AttributeUsage((System.AttributeTargets)384, Inherited = true, AllowMultiple = false)]
    public sealed class MinAttribute : PropertyAttribute
    {
        public readonly float min;
        public MinAttribute(float min) { this.min = min; }
    }

    [System.AttributeUsage((System.AttributeTargets)384, Inherited = true, AllowMultiple = false)]
    public sealed class MultilineAttribute : PropertyAttribute
    {
        public readonly int lines;
        public MultilineAttribute() { }
        public MultilineAttribute(int lines) { this.lines = lines; }
    }

    [System.AttributeUsage((System.AttributeTargets)384, Inherited = true, AllowMultiple = false)]
    public sealed class TextAreaAttribute : PropertyAttribute
    {
        public readonly int minLines;
        public readonly int maxLines;
        public TextAreaAttribute() { }
        public TextAreaAttribute(int minLines, int maxLines) { this.minLines = minLines; this.maxLines = maxLines; }
    }

    [System.AttributeUsage((System.AttributeTargets)384, Inherited = true, AllowMultiple = false)]
    public sealed class ColorUsageAttribute : PropertyAttribute
    {
        public readonly bool showAlpha;
        public readonly bool hdr;
        public readonly float minBrightness;
        public readonly float maxBrightness;
        public readonly float minExposureValue;
        public readonly float maxExposureValue;
        public ColorUsageAttribute(bool showAlpha) { this.showAlpha = showAlpha; }
        public ColorUsageAttribute(bool showAlpha, bool hdr) { this.showAlpha = showAlpha; this.hdr = hdr; }
        public ColorUsageAttribute(bool showAlpha, bool hdr, float minBrightness, float maxBrightness, float minExposureValue, float maxExposureValue)
        {
            this.showAlpha = showAlpha;
            this.hdr = hdr;
            this.minBrightness = minBrightness;
            this.maxBrightness = maxBrightness;
            this.minExposureValue = minExposureValue;
            this.maxExposureValue = maxExposureValue;
        }
    }

    [System.AttributeUsage((System.AttributeTargets)384, Inherited = true, AllowMultiple = false)]
    public sealed class DelayedAttribute : PropertyAttribute
    {
        public DelayedAttribute() { }
    }

    [System.AttributeUsage((System.AttributeTargets)4, Inherited = false)]
    public sealed class DisallowMultipleComponent : System.Attribute
    {
        public DisallowMultipleComponent() { }
    }

    [System.AttributeUsage((System.AttributeTargets)4, AllowMultiple = true)]
    public sealed class RequireComponent : System.Attribute
    {
        public Type m_Type0;
        public Type? m_Type1;
        public Type? m_Type2;
        public RequireComponent(Type requiredComponent) { m_Type0 = requiredComponent; }
        public RequireComponent(Type requiredComponent, Type requiredComponent2)
        {
            m_Type0 = requiredComponent;
            m_Type1 = requiredComponent2;
        }
        public RequireComponent(Type requiredComponent, Type requiredComponent2, Type requiredComponent3)
        {
            m_Type0 = requiredComponent;
            m_Type1 = requiredComponent2;
            m_Type2 = requiredComponent3;
        }
    }

    public sealed class AddComponentMenu : System.Attribute
    {
        public string componentMenu { get; }
        public int componentOrder { get; }
        public AddComponentMenu(string menuName) { componentMenu = menuName; }
        public AddComponentMenu(string menuName, int order) { componentMenu = menuName; componentOrder = order; }
    }

    [System.AttributeUsage((System.AttributeTargets)64, AllowMultiple = true)]
    public sealed class ContextMenu : System.Attribute
    {
        public readonly string menuItem;
        public readonly bool validate;
        public readonly int priority;
        public ContextMenu(string itemName) : this(itemName, false) { }
        public ContextMenu(string itemName, bool isValidateFunction) : this(itemName, isValidateFunction, 1000000) { }

        public ContextMenu(string itemName, bool isValidateFunction, int priority)
        {
            menuItem = itemName;
            validate = isValidateFunction;
            this.priority = priority;
        }
    }

    public sealed class ExecuteInEditMode : System.Attribute
    {
        public ExecuteInEditMode() { }
    }

    public sealed class ExecuteAlways : System.Attribute
    {
        public ExecuteAlways() { }
    }

    public sealed class HideInInspector : System.Attribute
    {
        public HideInInspector() { }
    }

    [System.AttributeUsage((System.AttributeTargets)4, AllowMultiple = false)]
    public class HelpURLAttribute : System.Attribute
    {
        public string URL { get; }
        public HelpURLAttribute(string url) { this.URL = url; }
    }

    [System.AttributeUsage((System.AttributeTargets)4)]
    public class DefaultExecutionOrder : System.Attribute
    {
        public int order { get; }
        public DefaultExecutionOrder(int order) { this.order = order; }
    }

    [System.AttributeUsage((System.AttributeTargets)4, Inherited = false)]
    public class ExcludeFromPresetAttribute : System.Attribute
    {
        public ExcludeFromPresetAttribute() { }
    }

    [System.AttributeUsage((System.AttributeTargets)4, Inherited = true, AllowMultiple = false)]
    public class SelectionBaseAttribute : System.Attribute
    {
        public SelectionBaseAttribute() { }
    }

    [System.AttributeUsage((System.AttributeTargets)4)]
    public sealed class PreferBinarySerialization : System.Attribute
    {
        public PreferBinarySerialization() { }
    }

    [System.AttributeUsage((System.AttributeTargets)4, Inherited = false)]
    public sealed class AssetFileNameExtensionAttribute : System.Attribute
    {
        public string preferredExtension { get; }
        public IEnumerable<string> otherExtensions { get; }

        public AssetFileNameExtensionAttribute(string preferredExtension, params string[] otherExtensions)
        {
            this.preferredExtension = preferredExtension;
            this.otherExtensions = otherExtensions;
        }
    }
}

namespace UnityEngine.Categorization
{
    [System.AttributeUsage((System.AttributeTargets)4, Inherited = false, AllowMultiple = false)]
    public sealed class ElementInfoAttribute : System.Attribute
    {
        public int Order { get; set; }
        public string Name { get; set; } = "";
        public ElementInfoAttribute() { }
    }

    [System.AttributeUsage((System.AttributeTargets)4, Inherited = false, AllowMultiple = false)]
    public sealed class CategoryInfoAttribute : System.Attribute
    {
        public int Order { get; set; }
        public string Name { get; set; } = "";
        public CategoryInfoAttribute() { }
    }
}

namespace UnityEngine.Rendering
{
    public enum SearchType
    {
        ProjectPath = 0,
        BuiltinPath = 1,
        BuiltinExtraPath = 2,
        ShaderName = 3,
    }

    [System.AttributeUsage((System.AttributeTargets)256, Inherited = true)]
    public abstract class ResourcePathsBaseAttribute : System.Attribute
    {
        public SearchType location { get; }
        public string[] paths { get; }
        public bool isField { get; }

        protected ResourcePathsBaseAttribute(string[] paths, bool isField, SearchType location)
        {
            this.paths = paths;
            this.isField = isField;
            this.location = location;
        }
    }

    public sealed class ResourcePathAttribute : ResourcePathsBaseAttribute
    {
        public ResourcePathAttribute(string path, SearchType location = SearchType.ProjectPath) : base([path], true, location) { }
    }

    public sealed class ResourcePathsAttribute : ResourcePathsBaseAttribute
    {
        public ResourcePathsAttribute(string[] paths, SearchType location = SearchType.ProjectPath) : base(paths, false, location) { }
    }

    public sealed class ResourceFormattedPathsAttribute : ResourcePathsBaseAttribute
    {
        /// <summary>The paths <paramref name="pathFormat"/> gives for each index from
        /// <paramref name="rangeMin"/> up to, not including, <paramref name="rangeMax"/>.</summary>
        public ResourceFormattedPathsAttribute(string pathFormat, int rangeMin, int rangeMax, SearchType location = SearchType.ProjectPath)
            : base(Enumerable.Range(rangeMin, Math.Max(rangeMax - rangeMin, 0))
                .Select(i => string.Format(System.Globalization.CultureInfo.InvariantCulture, pathFormat, i)).ToArray(), false, location) { }
    }

    [System.AttributeUsage((System.AttributeTargets)256, AllowMultiple = false)]
    public class RecreatePipelineOnChangeAttribute : System.Attribute
    {
        public RecreatePipelineOnChangeAttribute() { }
    }

    [System.AttributeUsage((System.AttributeTargets)4, AllowMultiple = false)]
    public class SupportedOnRenderPipelineAttribute : System.Attribute
    {
        public Type[] renderPipelineTypes { get; }

        /// <summary>No render pipeline asset is active in the engine yet, which is Unity's
        /// built-in renderer: a type named here is supported only when it names none.</summary>
        public bool isSupportedOnCurrentPipeline => renderPipelineTypes.Length == 0;

        public SupportedOnRenderPipelineAttribute(Type renderPipeline) { renderPipelineTypes = [renderPipeline]; }
        public SupportedOnRenderPipelineAttribute(params Type[] renderPipeline) { renderPipelineTypes = renderPipeline; }
    }
}

namespace UnityEngine.Scripting
{
    [System.AttributeUsage((System.AttributeTargets)1)]
    public class AlwaysLinkAssemblyAttribute : System.Attribute
    {
        public AlwaysLinkAssemblyAttribute() { }
    }

    [System.AttributeUsage((System.AttributeTargets)6141, Inherited = false)]
    public class PreserveAttribute : System.Attribute
    {
        public PreserveAttribute() { }
    }

    [System.AttributeUsage((System.AttributeTargets)4, AllowMultiple = false)]
    public class RequireAttributeUsagesAttribute : System.Attribute
    {
        public RequireAttributeUsagesAttribute() { }
    }

    [System.AttributeUsage((System.AttributeTargets)1024, AllowMultiple = false)]
    public class RequireImplementorsAttribute : System.Attribute
    {
        public RequireImplementorsAttribute() { }
    }
}

namespace UnityEngine.Scripting.APIUpdating
{
    [System.AttributeUsage((System.AttributeTargets)5148)]
    public class MovedFromAttribute : System.Attribute
    {
        public bool IsInDifferentAssembly { get; }
        public MovedFromAttribute(bool autoUpdateAPI, string? sourceNamespace, string? sourceAssembly, string? sourceClassName)
        {
            IsInDifferentAssembly = sourceAssembly is not null;
        }

        public MovedFromAttribute(string sourceNamespace) : this(true, sourceNamespace, null, null) { }
    }
}

namespace UnityEngine.Serialization
{
    [System.AttributeUsage((System.AttributeTargets)256, AllowMultiple = true, Inherited = false)]
    public class FormerlySerializedAsAttribute : System.Attribute
    {
        public string oldName { get; }
        public FormerlySerializedAsAttribute(string oldName) { this.oldName = oldName; }
    }
}
