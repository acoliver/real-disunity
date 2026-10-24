namespace UnityEngine;

public enum ForceMode2D { Force, Impulse }
public enum RigidbodyType2D { Dynamic, Kinematic, Static }
public class Rigidbody2D : Component
{
    public Vector2 velocity { get { var v = Fields.GetProperty("velocity"); return new(v[0].GetSingle(), v[1].GetSingle()); } set => SetField("velocity", new[] { value.x, value.y }); }
    public Vector2 linearVelocity { get => velocity; set => velocity = value; }
    public float angularVelocity { get => Fields.GetProperty("angular_velocity").GetSingle(); set => SetField("angular_velocity", value); }
    public Vector2 position { get => transform.position; set { var p = transform.position; transform.position = new(value.x, value.y, p.z); } }
    public float gravityScale { get => Fields.GetProperty("gravity_scale").GetSingle(); set => SetField("gravity_scale", value); }
    public float mass { get => Fields.GetProperty("mass").GetSingle(); set => SetField("mass", value); }
    public bool simulated { get => Fields.GetProperty("simulated").GetBoolean(); set => SetField("simulated", value); }
    public RigidbodyType2D bodyType { get => Enum.Parse<RigidbodyType2D>(Fields.GetProperty("body_type").GetString()!, true); set => SetField("body_type", value.ToString().ToLowerInvariant()); }
    public void MovePosition(Vector2 position) => NativeWorld.Request("body.move", ("component", NativeHandle), ("position", new[] { position.x, position.y })).Dispose();
    public void Sleep() => NativeWorld.Request("body.sleep", ("component", NativeHandle)).Dispose();
    public void WakeUp() => NativeWorld.Request("body.wake", ("component", NativeHandle)).Dispose();
    public bool IsSleeping() => Fields.GetProperty("sleeping").GetBoolean();
    public void AddForce(Vector2 force, ForceMode2D mode = ForceMode2D.Force) => NativeWorld.Request("body.force", ("component", NativeHandle), ("force", new[] { force.x, force.y }), ("mode", mode.ToString().ToLowerInvariant())).Dispose();
}
public class Collider2D : Component
{
    public bool isTrigger { get => Fields.GetProperty("is_trigger").GetBoolean(); set => SetField("is_trigger", value); }
    public Vector2 offset { get { var p = Fields.GetProperty("offset"); return new(p[0].GetSingle(), p[1].GetSingle()); } set => SetField("offset", new[] { value.x, value.y }); }
    public Rigidbody2D? attachedRigidbody
    {
        get
        {
            for (Transform? current = transform; current is not null; current = current.parent)
            {
                var body = current.GetComponent<Rigidbody2D>();
                if (body is not null) return body;
            }
            return null;
        }
    }
}
public class BoxCollider2D : Collider2D
{
    public Vector2 size { get { var v = Fields.GetProperty("size"); return new(v[0].GetSingle(), v[1].GetSingle()); } set => SetField("size", new[] { value.x, value.y }); }
}
public class CircleCollider2D : Collider2D
{
    public float radius { get => Fields.GetProperty("radius").GetSingle(); set => SetField("radius", value); }
}
public class PolygonCollider2D : Collider2D { }
public readonly struct ContactPoint2D(Vector2 point, Vector2 normal)
{
    public Vector2 point { get; } = point;
    public Vector2 normal { get; } = normal;
}
public sealed class Collision2D(Collider2D other, Collider2D self, Vector2 point, Vector2 normal)
{
    public Collider2D collider { get; } = other;
    public Collider2D otherCollider { get; } = self;
    public GameObject gameObject => collider.gameObject;
    public Transform transform => gameObject.transform;
    public Rigidbody2D? rigidbody => collider.attachedRigidbody;
    public ContactPoint2D[] contacts => [new(point, normal)];
    public int contactCount => 1;
    public ContactPoint2D GetContact(int index) => index == 0 ? new(point, normal) : throw new ArgumentOutOfRangeException(nameof(index));
}
