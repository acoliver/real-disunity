namespace UnityEngine;

/// <summary>
/// A 3D rigid body. It cannot be attached to a GameObject (see <see cref="UnsupportedComponents"/>), so
/// an instance is never simulated: its properties are stored with Unity's defaults and read back, and
/// the force and torque methods have no effect because nothing integrates them.
/// </summary>
public partial class Rigidbody : Component
{
    private RigidbodyConstraints _constraints;
    private bool _sleeping;

    public Vector3 linearVelocity { get; set; }
    public Vector3 angularVelocity { get; set; }
    public float linearDamping { get; set; }
    public float angularDamping { get; set; } = 0.05f;
    public float mass { get; set; } = 1f;
    public bool useGravity { get; set; } = true;
    public float maxDepenetrationVelocity { get; set; } = Physics.defaultMaxDepenetrationVelocity;
    public bool isKinematic { get; set; }
    public CollisionDetectionMode collisionDetectionMode { get; set; } = CollisionDetectionMode.Discrete;
    public bool automaticCenterOfMass { get; set; } = true;
    public Vector3 centerOfMass { get; set; }
    public bool automaticInertiaTensor { get; set; } = true;
    public Quaternion inertiaTensorRotation { get; set; } = Quaternion.identity;
    public Vector3 inertiaTensor { get; set; } = Vector3.one;
    public bool detectCollisions { get; set; } = true;
    public Vector3 position { get; set; }
    public Quaternion rotation { get; set; } = Quaternion.identity;
    public RigidbodyInterpolation interpolation { get; set; } = RigidbodyInterpolation.None;
    public int solverIterations { get; set; } = Physics.defaultSolverIterations;
    public int solverVelocityIterations { get; set; } = Physics.defaultSolverVelocityIterations;
    public float sleepThreshold { get; set; } = Physics.sleepThreshold;
    public float maxAngularVelocity { get; set; } = Physics.defaultMaxAngularSpeed;
    public float maxLinearVelocity { get; set; } = 1e7f;
    public LayerMask excludeLayers { get; set; }
    public LayerMask includeLayers { get; set; }

    public RigidbodyConstraints constraints
    {
        get => _constraints;
        set => _constraints = value;
    }

    /// <summary>Whether all three rotation axes are frozen; setting it adds or removes those constraints.</summary>
    public bool freezeRotation
    {
        get => (_constraints & RigidbodyConstraints.FreezeRotation) == RigidbodyConstraints.FreezeRotation;
        set => _constraints = value
            ? _constraints | RigidbodyConstraints.FreezeRotation
            : _constraints & ~RigidbodyConstraints.FreezeRotation;
    }

    public Vector3 worldCenterOfMass => position + rotation * centerOfMass;

    public float drag { get => linearDamping; set => linearDamping = value; }
    public float angularDrag { get => angularDamping; set => angularDamping = value; }
    public Vector3 velocity { get => linearVelocity; set => linearVelocity = value; }

    public void MovePosition(Vector3 position) => this.position = position;
    public void MoveRotation(Quaternion rotation) => this.rotation = rotation;

    public void Move(Vector3 position, Quaternion rotation)
    {
        this.position = position;
        this.rotation = rotation;
    }

    public void Sleep() => _sleeping = true;
    public bool IsSleeping() => _sleeping;
    public void WakeUp() => _sleeping = false;
    public void ResetCenterOfMass() => centerOfMass = Vector3.zero;
    public void ResetInertiaTensor() => inertiaTensor = Vector3.one;
    public void PublishTransform() { }

    public Vector3 GetPointVelocity(Vector3 worldPoint) =>
        linearVelocity + Vector3.Cross(angularVelocity, worldPoint - worldCenterOfMass);

    public Vector3 GetRelativePointVelocity(Vector3 relativePoint) => GetPointVelocity(position + rotation * relativePoint);
}
