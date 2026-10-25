using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private static readonly Dictionary<ulong, Component> NativeComponents = new();
    internal static Collider2D ResolveCollider(ulong handle) => (Collider2D)NativeComponents[handle];
    internal static unsafe T AttachComponent<T>(GameObject owner, T value) where T : Component
    {
        if (value is MonoBehaviour script) return (T)(Component)Attach(owner, script);
        if (value is not NativeComponent && value is not RectTransform) throw new NotSupportedException($"component {typeof(T)}");
        ulong handle;
        if (value is Canvas || value is RectTransform)
        {
            if (value is RectTransform && owner.transform is RectTransform) throw new InvalidOperationException("object already has a RectTransform");
            ThrowIfError(NativeBindings.disunity_ui_create(WorldHandle, owner.NativeHandle, value is Canvas ? 1u : 0u, 0, &handle));
        }
        else
        {
            uint kind = value switch { Rigidbody2D => 0, BoxCollider2D => 1, CircleCollider2D => 2, _ => throw new NotSupportedException($"component {typeof(T)}") };
            ThrowIfError(NativeBindings.disunity_physics_create(WorldHandle, owner.NativeHandle, kind, &handle));
        }
        value.Handle = handle;
        value.gameObject = owner;
        NativeComponents.Add(handle, value);
        if (value is RectTransform rect) owner.Transform = rect;
        return value;
    }

    private static void DispatchContact(MonoBehaviour script, DisunityCallback payload)
    {
        string[] phases = ["Enter2D", "Stay2D", "Exit2D"];
        var self = ResolveCollider(payload.collider);
        var other = ResolveCollider(payload.other);
        object argument = payload.trigger == 1 ? other : new Collision2D(other, self,
            new(payload.point_x, payload.point_y), new(payload.normal_x, payload.normal_y));
        ScriptMessages.Invoke(script, (payload.trigger == 1 ? "OnTrigger" : "OnCollision") + phases[payload.phase], argument);
    }
}
