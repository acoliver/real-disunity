namespace UnityEngine;

public partial class Rigidbody
{
    public Vector3 GetAccumulatedForce(float step) => Vector3.zero;
    public Vector3 GetAccumulatedForce() => Vector3.zero;
    public Vector3 GetAccumulatedTorque(float step) => Vector3.zero;
    public Vector3 GetAccumulatedTorque() => Vector3.zero;

    public void AddForce(Vector3 force, ForceMode mode) { }
    public void AddForce(Vector3 force) { }
    public void AddForce(float x, float y, float z, ForceMode mode) { }
    public void AddForce(float x, float y, float z) { }
    public void AddRelativeForce(Vector3 force, ForceMode mode) { }
    public void AddRelativeForce(Vector3 force) { }
    public void AddRelativeForce(float x, float y, float z, ForceMode mode) { }
    public void AddRelativeForce(float x, float y, float z) { }
    public void AddTorque(Vector3 torque, ForceMode mode) { }
    public void AddTorque(Vector3 torque) { }
    public void AddTorque(float x, float y, float z, ForceMode mode) { }
    public void AddTorque(float x, float y, float z) { }
    public void AddRelativeTorque(Vector3 torque, ForceMode mode) { }
    public void AddRelativeTorque(Vector3 torque) { }
    public void AddRelativeTorque(float x, float y, float z, ForceMode mode) { }
    public void AddRelativeTorque(float x, float y, float z) { }
    public void AddForceAtPosition(Vector3 force, Vector3 position, ForceMode mode) { }
    public void AddForceAtPosition(Vector3 force, Vector3 position) { }
    public void AddExplosionForce(float explosionForce, Vector3 explosionPosition, float explosionRadius, float upwardsModifier, ForceMode mode) { }
    public void AddExplosionForce(float explosionForce, Vector3 explosionPosition, float explosionRadius, float upwardsModifier) { }
    public void AddExplosionForce(float explosionForce, Vector3 explosionPosition, float explosionRadius) { }

    /// <summary>Density sets mass from the attached colliders' volume; with none attached the mass is unchanged.</summary>
    public void SetDensity(float density) { }

    /// <summary>With no colliders the bounding box is the body's own position.</summary>
    public Vector3 ClosestPointOnBounds(Vector3 position) => this.position;

    public bool SweepTest(Vector3 direction, out RaycastHit hitInfo, float maxDistance, QueryTriggerInteraction queryTriggerInteraction) { hitInfo = default; return false; }
    public bool SweepTest(Vector3 direction, out RaycastHit hitInfo, float maxDistance) { hitInfo = default; return false; }
    public bool SweepTest(Vector3 direction, out RaycastHit hitInfo) { hitInfo = default; return false; }
    public RaycastHit[] SweepTestAll(Vector3 direction, float maxDistance, QueryTriggerInteraction queryTriggerInteraction) => [];
    public RaycastHit[] SweepTestAll(Vector3 direction, float maxDistance) => [];
    public RaycastHit[] SweepTestAll(Vector3 direction) => [];
}
