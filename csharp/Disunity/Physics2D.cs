using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

public record struct Vector2(float x, float y)
{
    public static Vector2 zero => new(0, 0);
    public static Vector2 one => new(1, 1);
    public static Vector2 up => new(0, 1);
    public static Vector2 right => new(1, 0);
    public float sqrMagnitude => x * x + y * y;
    public float magnitude => MathF.Sqrt(x * x + y * y);
    public Vector2 normalized => magnitude == 0 ? zero : this / magnitude;
    public static Vector2 operator +(Vector2 a, Vector2 b) => new(a.x + b.x, a.y + b.y);
    public static Vector2 operator -(Vector2 a, Vector2 b) => new(a.x - b.x, a.y - b.y);
    public static Vector2 operator -(Vector2 a) => new(-a.x, -a.y);
    public static Vector2 operator *(Vector2 a, float b) => new(a.x * b, a.y * b);
    public static Vector2 operator /(Vector2 a, float b) => new(a.x / b, a.y / b);
}
public abstract class NativeComponent : Component
{
}
[Flags]
public enum RigidbodyConstraints2D
{
    None = 0, FreezePositionX = 1, FreezePositionY = 2, FreezePosition = 3,
    FreezeRotation = 4, FreezeAll = 7
}
public enum RigidbodyType2D { Dynamic, Kinematic, Static }
public sealed class Rigidbody2D : NativeComponent
{
    public PhysicsMaterial2D? sharedMaterial { get => PhysicsMaterial2D.Get(Handle); set => PhysicsMaterial2D.Set(Handle, value); }
    private unsafe DisunityBody2d Read()
    {
        DisunityBody2d value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_body_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    private unsafe void Write(DisunityBody2d value) => DisunityHost.ThrowIfError(NativeBindings.disunity_body_write(DisunityHost.WorldHandle, Handle, &value));
    public Vector2 velocity { get { var b = Read(); return new(b.velocity_x, b.velocity_y); } set { var b = Read(); b.velocity_x = value.x; b.velocity_y = value.y; Write(b); } }
    public Vector2 linearVelocity { get => velocity; set => velocity = value; }
    public float angularVelocity { get => Read().angular_velocity * (180 / MathF.PI); set { var b = Read(); b.angular_velocity = value * (MathF.PI / 180); Write(b); } }
    public float gravityScale { get => Read().gravity_scale; set { var b = Read(); b.gravity_scale = value; Write(b); } }
    public float mass { get => Read().mass; set { var b = Read(); b.mass = value; Write(b); } }
    public bool simulated { get => Read().enabled == 1; set { var b = Read(); b.enabled = value ? 1u : 0u; Write(b); } }
    public bool freezeRotation { get => Read().freeze_rotation == 1; set { var b = Read(); b.freeze_rotation = value ? 1u : 0u; Write(b); } }
    public RigidbodyConstraints2D constraints
    {
        get { var b = Read(); return (RigidbodyConstraints2D)(b.freeze_position | (b.freeze_rotation << 2)); }
        set
        {
            if (((uint)value & ~7u) != 0) throw new ArgumentOutOfRangeException(nameof(value));
            var b = Read();
            b.freeze_position = (uint)value & 3;
            b.freeze_rotation = ((uint)value >> 2) & 1;
            Write(b);
        }
    }
    public RigidbodyType2D bodyType { get => (RigidbodyType2D)Read().kind; set { var b = Read(); b.kind = (uint)value; Write(b); } }
    public void Sleep() => DisunityHost.ThrowIfError(NativeBindings.disunity_body_set_sleeping(DisunityHost.WorldHandle, Handle, 1));
    public void WakeUp() => DisunityHost.ThrowIfError(NativeBindings.disunity_body_set_sleeping(DisunityHost.WorldHandle, Handle, 0));
    public unsafe bool IsSleeping()
    {
        uint sleeping;
        DisunityHost.ThrowIfError(NativeBindings.disunity_body_sleeping(DisunityHost.WorldHandle, Handle, &sleeping));
        return sleeping == 1;
    }
    public void MovePosition(Vector2 position) => DisunityHost.ThrowIfError(NativeBindings.disunity_body_move_position(DisunityHost.WorldHandle, Handle, position.x, position.y));
    public Vector2 position { get { var p = transform.position; return new(p.X, p.Y); } set { var p = transform.position; transform.position = new(value.x, value.y, p.Z); } }
}
public abstract class Collider2D : NativeComponent
{
    public PhysicsMaterial2D? sharedMaterial { get => PhysicsMaterial2D.Get(Handle); set => PhysicsMaterial2D.Set(Handle, value); }
    internal unsafe DisunityCollider2d Read()
    {
        DisunityCollider2d value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_collider_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    internal unsafe void Write(DisunityCollider2d value) => DisunityHost.ThrowIfError(NativeBindings.disunity_collider_write(DisunityHost.WorldHandle, Handle, &value));
    public bool enabled { get => Read().enabled == 1; set { var c = Read(); c.enabled = value ? 1u : 0u; Write(c); } }
    public bool isTrigger { get => Read().trigger == 1; set { var c = Read(); c.trigger = value ? 1u : 0u; Write(c); } }
    public Vector2 offset { get { var c = Read(); return new(c.offset_x, c.offset_y); } set { var c = Read(); c.offset_x = value.x; c.offset_y = value.y; Write(c); } }
    public Rigidbody2D? attachedRigidbody
    {
        get
        {
            for (var current = transform; current != null; current = current.parent)
                if (current.GetComponent<Rigidbody2D>() is { } body) return body;
            return null;
        }
    }
}
public sealed class BoxCollider2D : Collider2D
{
    public Vector2 size { get { var c = Read(); return new(c.size_x, c.size_y); } set { var c = Read(); c.size_x = value.x; c.size_y = value.y; Write(c); } }
}
public sealed class CircleCollider2D : Collider2D
{
    public float radius { get => Read().radius; set { var c = Read(); c.radius = value; Write(c); } }
}
public readonly record struct ContactPoint2D(Vector2 point, Vector2 normal);
public sealed class Collision2D(Collider2D other, Collider2D self, Vector2 point, Vector2 normal)
{
    public Collider2D collider { get; } = other;
    public Collider2D otherCollider { get; } = self;
    public GameObject gameObject => collider.gameObject;
    public Transform transform => collider.transform;
    public Rigidbody2D? rigidbody => collider.attachedRigidbody;
    public ContactPoint2D[] contacts => [new(point, normal)];
    public int contactCount => 1;
    public ContactPoint2D GetContact(int index) => index == 0 ? new(point, normal) : throw new ArgumentOutOfRangeException(nameof(index));
}
public readonly struct RaycastHit2D
{
    public Collider2D? collider { get; init; }
    public Vector2 point { get; init; }
    public Vector2 normal { get; init; }
    public float distance { get; init; }
    public static implicit operator bool(RaycastHit2D hit) => hit.collider != null;
}
public static class Physics2D
{
    public static unsafe RaycastHit2D Raycast(Vector2 origin, Vector2 direction, float distance = float.MaxValue, int layerMask = -1)
    {
        DisunityRayHit hit;
        DisunityHost.ThrowIfError(NativeBindings.disunity_physics_raycast(DisunityHost.WorldHandle, origin.x, origin.y, direction.x, direction.y, distance, unchecked((uint)layerMask), &hit));
        return new RaycastHit2D { collider = hit.collider == 0 ? null : DisunityHost.ResolveCollider(hit.collider), point = new(hit.point_x, hit.point_y), normal = new(hit.normal_x, hit.normal_y), distance = hit.distance };
    }
    public static unsafe Collider2D? OverlapCircle(Vector2 point, float radius, int layerMask = -1)
    {
        ulong handle;
        DisunityHost.ThrowIfError(NativeBindings.disunity_physics_overlap_circle(DisunityHost.WorldHandle, point.x, point.y, radius, unchecked((uint)layerMask), &handle));
        return handle == 0 ? null : DisunityHost.ResolveCollider(handle);
    }
}
