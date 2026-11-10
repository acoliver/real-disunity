// The engine's attribute classes, written from its own knowledge of what each one holds and checked against the player.

namespace UnityEngine
{
    /// <summary>Base class of the attributes that decorate serialized fields (Header, Range, Space, Tooltip, ColorUsage).</summary>
    [AttributeUsage(AttributeTargets.All, Inherited = true, AllowMultiple = true)]
    public abstract class PropertyAttribute : Attribute
    {
        public int order { get; set; }
    }

    [AttributeUsage(AttributeTargets.Field, Inherited = true, AllowMultiple = true)]
    public class SpaceAttribute : PropertyAttribute
    {
        public readonly float height;

        public SpaceAttribute() => height = 8f;

        public SpaceAttribute(float height) => this.height = height;
    }

    [AttributeUsage(AttributeTargets.Field | AttributeTargets.Property | AttributeTargets.Class | AttributeTargets.Struct, Inherited = true, AllowMultiple = true)]
    public class TooltipAttribute(string tooltip) : PropertyAttribute
    {
        public readonly string tooltip = tooltip;
    }

    [AttributeUsage(AttributeTargets.Field, Inherited = true, AllowMultiple = true)]
    public sealed class TextAreaAttribute : PropertyAttribute
    {
        public readonly int minLines;
        public readonly int maxLines;

        public TextAreaAttribute()
        {
            minLines = 3;
            maxLines = 3;
        }

        public TextAreaAttribute(int minLines, int maxLines)
        {
            this.minLines = minLines;
            this.maxLines = maxLines;
        }
    }

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Enum | AttributeTargets.Interface | AttributeTargets.Delegate, Inherited = false, AllowMultiple = false)]
    public class HelpURLAttribute(string url) : Attribute
    {
        public string URL { get; } = url;
    }

    [AttributeUsage(AttributeTargets.Field | AttributeTargets.Property, Inherited = true, AllowMultiple = true)]
    public sealed class DelayedAttribute : PropertyAttribute;

    [AttributeUsage(AttributeTargets.Field | AttributeTargets.Property, Inherited = true, AllowMultiple = true)]
    public sealed class ColorUsageAttribute : PropertyAttribute
    {
        public readonly bool showAlpha = true;
        public readonly bool hdr;

        public ColorUsageAttribute(bool showAlpha) => this.showAlpha = showAlpha;

        public ColorUsageAttribute(bool showAlpha, bool hdr)
        {
            this.showAlpha = showAlpha;
            this.hdr = hdr;
        }
    }

    /// <summary>A script's required components; <c>AddComponent</c> adds them first. The fields hold up to three types.</summary>
    [AttributeUsage(AttributeTargets.Class, Inherited = true, AllowMultiple = true)]
    public sealed class RequireComponent : Attribute
    {
        public Type? m_Type0;
        public Type? m_Type1;
        public Type? m_Type2;

        public RequireComponent(Type requiredComponent) => m_Type0 = requiredComponent;

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

    [AttributeUsage(AttributeTargets.Class, Inherited = false, AllowMultiple = false)]
    public sealed class AddComponentMenu : Attribute
    {
        public AddComponentMenu(string addComponentMenu) => componentMenu = addComponentMenu;

        public AddComponentMenu(string addComponentMenu, int order)
        {
            componentMenu = addComponentMenu;
            componentOrder = order;
        }

        public string componentMenu { get; }

        public int componentOrder { get; }
    }

    [AttributeUsage(AttributeTargets.Method, Inherited = true, AllowMultiple = true)]
    public sealed class ContextMenu : Attribute
    {
        public ContextMenu(string itemName) : this(itemName, false) { }

        public ContextMenu(string itemName, bool isValidateFunction) : this(itemName, isValidateFunction, 1000000) { }

        public ContextMenu(string itemName, bool isValidateFunction, int priority)
        {
            menuItem = itemName;
            validate = isValidateFunction;
            this.priority = priority;
        }

        public string menuItem { get; }

        public bool validate { get; }

        public int priority { get; }
    }

    [AttributeUsage(AttributeTargets.Class, Inherited = true, AllowMultiple = false)]
    public sealed class DefaultExecutionOrder(int order) : Attribute
    {
        public int order { get; } = order;
    }

    [AttributeUsage(AttributeTargets.Class, Inherited = true, AllowMultiple = false)]
    public sealed class ExcludeFromPresetAttribute : Attribute;

    [AttributeUsage(AttributeTargets.Class, Inherited = true, AllowMultiple = false)]
    public sealed class SelectionBaseAttribute : Attribute;

    [AttributeUsage(AttributeTargets.Class, Inherited = true, AllowMultiple = false)]
    public sealed class DisallowMultipleComponent : Attribute;

    [AttributeUsage(AttributeTargets.Class, Inherited = true, AllowMultiple = false)]
    public sealed class ExecuteAlways : Attribute;

    [AttributeUsage(AttributeTargets.Class, Inherited = false, AllowMultiple = false)]
    public sealed class ExecuteInEditMode : Attribute;

    [AttributeUsage(AttributeTargets.Field, Inherited = true, AllowMultiple = false)]
    public sealed class HideInInspector : Attribute;

    [AttributeUsage(AttributeTargets.Class, Inherited = true, AllowMultiple = false)]
    public sealed class ImageEffectAllowedInSceneView : Attribute;

    [AttributeUsage(AttributeTargets.Method, Inherited = true, AllowMultiple = false)]
    public sealed class ImageEffectOpaque : Attribute;

    [AttributeUsage(AttributeTargets.Method, Inherited = true, AllowMultiple = false)]
    public sealed class ImageEffectTransformsToLDR : Attribute;
}

namespace UnityEngine.Serialization
{
    /// <summary>The names a field had before it was renamed, so older serialized data still finds it.</summary>
    [AttributeUsage(AttributeTargets.Field | AttributeTargets.Property, AllowMultiple = true)]
    public class FormerlySerializedAsAttribute(string oldName) : Attribute
    {
        public string oldName { get; } = oldName;
    }
}

namespace UnityEngine.Scripting
{
    [AttributeUsage(AttributeTargets.All, Inherited = false)]
    public class PreserveAttribute : Attribute;

    [AttributeUsage(AttributeTargets.Assembly)]
    public class AlwaysLinkAssemblyAttribute : Attribute;
}

namespace UnityEngine.Scripting.APIUpdating
{
    /// <summary>Where a type lived before it moved: its old namespace, assembly and name.</summary>
    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Enum | AttributeTargets.Interface | AttributeTargets.Delegate, AllowMultiple = true, Inherited = false)]
    public class MovedFromAttribute : Attribute
    {
        public MovedFromAttribute(string sourceNamespace) : this(true, sourceNamespace) { }

        public MovedFromAttribute(bool autoUpdateAPI, string? sourceNamespace = null, string? sourceAssembly = null, string? sourceClassName = null)
        {
            AutoUpdateAPI = autoUpdateAPI;
            Namespace = sourceNamespace;
            Assembly = sourceAssembly;
            ClassName = sourceClassName;
        }

        public string? ClassName { get; }

        public string? Assembly { get; }

        public string? Namespace { get; }

        public bool AutoUpdateAPI { get; }
    }
}

namespace UnityEngine.Rendering
{
    /// <summary>A render pipeline resource's location: a path in the project, or the name of a shader.</summary>
    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Field, AllowMultiple = false)]
    public sealed class ResourcePathAttribute : Attribute
    {
        public enum SearchType
        {
            ProjectPath,
            ShaderName,
        }

        public ResourcePathAttribute(string path, SearchType location = SearchType.ProjectPath)
        {
            this.path = path;
            this.location = location;
        }

        public string path { get; }

        public SearchType location { get; }
    }

    /// <summary>A resource's locations, one for each number in a range, from <paramref name="rangeMin"/> up to but not including <paramref name="rangeMax"/>.</summary>
    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Field, AllowMultiple = false)]
    public sealed class ResourceFormattedPathsAttribute : Attribute
    {
        public ResourceFormattedPathsAttribute(string pathFormat, int rangeMin, int rangeMax, ResourcePathAttribute.SearchType location = ResourcePathAttribute.SearchType.ProjectPath)
        {
            paths = Enumerable.Range(rangeMin, Math.Max(0, rangeMax - rangeMin)).Select(i => string.Format(pathFormat, i)).ToArray();
            this.location = location;
        }

        public string[] paths { get; }

        public ResourcePathAttribute.SearchType location { get; }
    }

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Field, AllowMultiple = false)]
    public sealed class ResourcePathsAttribute(string[] paths, ResourcePathAttribute.SearchType location = ResourcePathAttribute.SearchType.ProjectPath) : Attribute
    {
        public string[] paths { get; } = paths;

        public ResourcePathAttribute.SearchType location { get; } = location;
    }

    /// <summary>The render pipelines a type works with; a type without it works with every pipeline.</summary>
    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct, AllowMultiple = false)]
    public class SupportedOnRenderPipelineAttribute : Attribute
    {
        public SupportedOnRenderPipelineAttribute(Type renderPipeline) => renderPipelineTypes = [renderPipeline];

        public SupportedOnRenderPipelineAttribute(params Type[] renderPipelineTypes) => this.renderPipelineTypes = renderPipelineTypes;

        public Type[] renderPipelineTypes { get; }

        /// <summary>
        /// True for a type without the attribute, and otherwise when the pipeline is one of the types the attribute names
        /// or derives from one of them.
        /// </summary>
        public static bool IsTypeSupportedOnRenderPipeline(Type type, Type renderPipelineType)
        {
            ArgumentNullException.ThrowIfNull(type);
            ArgumentNullException.ThrowIfNull(renderPipelineType);
            var attribute = (SupportedOnRenderPipelineAttribute?)GetCustomAttribute(type, typeof(SupportedOnRenderPipelineAttribute), true);
            return attribute is null || attribute.renderPipelineTypes.Any(named => named.IsAssignableFrom(renderPipelineType));
        }
    }

    [AttributeUsage(AttributeTargets.Field | AttributeTargets.Property, AllowMultiple = false)]
    public sealed class RecreatePipelineOnChangeAttribute : Attribute;
}
