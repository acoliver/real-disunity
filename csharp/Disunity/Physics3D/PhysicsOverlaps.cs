namespace UnityEngine;

public partial class Physics
{
    public static Collider[] OverlapCapsule(Vector3 point0, Vector3 point1, float radius, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => [];
    public static Collider[] OverlapCapsule(Vector3 point0, Vector3 point1, float radius, int layerMask) => [];
    public static Collider[] OverlapCapsule(Vector3 point0, Vector3 point1, float radius) => [];
    public static Collider[] OverlapSphere(Vector3 position, float radius, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => [];
    public static Collider[] OverlapSphere(Vector3 position, float radius, int layerMask) => [];
    public static Collider[] OverlapSphere(Vector3 position, float radius) => [];
    public static int OverlapSphereNonAlloc(Vector3 position, float radius, Collider[] results, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => 0;
    public static int OverlapSphereNonAlloc(Vector3 position, float radius, Collider[] results, int layerMask) => 0;
    public static int OverlapSphereNonAlloc(Vector3 position, float radius, Collider[] results) => 0;
    public static bool CheckSphere(Vector3 position, float radius, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => false;
    public static bool CheckSphere(Vector3 position, float radius, int layerMask) => false;
    public static bool CheckSphere(Vector3 position, float radius) => false;
    public static bool CheckCapsule(Vector3 start, Vector3 end, float radius, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => false;
    public static bool CheckCapsule(Vector3 start, Vector3 end, float radius, int layerMask) => false;
    public static bool CheckCapsule(Vector3 start, Vector3 end, float radius) => false;
    public static bool CheckBox(Vector3 center, Vector3 halfExtents, Quaternion orientation, int layermask, QueryTriggerInteraction queryTriggerInteraction) => false;
    public static bool CheckBox(Vector3 center, Vector3 halfExtents, Quaternion orientation, int layerMask) => false;
    public static bool CheckBox(Vector3 center, Vector3 halfExtents, Quaternion orientation) => false;
    public static bool CheckBox(Vector3 center, Vector3 halfExtents) => false;
    public static Collider[] OverlapBox(Vector3 center, Vector3 halfExtents, Quaternion orientation, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => [];
    public static Collider[] OverlapBox(Vector3 center, Vector3 halfExtents, Quaternion orientation, int layerMask) => [];
    public static Collider[] OverlapBox(Vector3 center, Vector3 halfExtents, Quaternion orientation) => [];
    public static Collider[] OverlapBox(Vector3 center, Vector3 halfExtents) => [];
    public static int OverlapBoxNonAlloc(Vector3 center, Vector3 halfExtents, Collider[] results, Quaternion orientation, int mask, QueryTriggerInteraction queryTriggerInteraction) => 0;
    public static int OverlapBoxNonAlloc(Vector3 center, Vector3 halfExtents, Collider[] results, Quaternion orientation, int mask) => 0;
    public static int OverlapBoxNonAlloc(Vector3 center, Vector3 halfExtents, Collider[] results, Quaternion orientation) => 0;
    public static int OverlapBoxNonAlloc(Vector3 center, Vector3 halfExtents, Collider[] results) => 0;
    public static int OverlapCapsuleNonAlloc(Vector3 point0, Vector3 point1, float radius, Collider[] results, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => 0;
    public static int OverlapCapsuleNonAlloc(Vector3 point0, Vector3 point1, float radius, Collider[] results, int layerMask) => 0;
    public static int OverlapCapsuleNonAlloc(Vector3 point0, Vector3 point1, float radius, Collider[] results) => 0;
}
