namespace UnityEngine;

public partial class Physics
{
    public static bool Raycast(Vector3 origin, Vector3 direction, float maxDistance, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => false;
    public static bool Raycast(Vector3 origin, Vector3 direction, float maxDistance, int layerMask) => false;
    public static bool Raycast(Vector3 origin, Vector3 direction, float maxDistance) => false;
    public static bool Raycast(Vector3 origin, Vector3 direction) => false;
    public static bool Raycast(Vector3 origin, Vector3 direction, out RaycastHit hitInfo, float maxDistance, int layerMask, QueryTriggerInteraction queryTriggerInteraction) { hitInfo = default; return false; }
    public static bool Raycast(Vector3 origin, Vector3 direction, out RaycastHit hitInfo, float maxDistance, int layerMask) { hitInfo = default; return false; }
    public static bool Raycast(Vector3 origin, Vector3 direction, out RaycastHit hitInfo, float maxDistance) { hitInfo = default; return false; }
    public static bool Raycast(Vector3 origin, Vector3 direction, out RaycastHit hitInfo) { hitInfo = default; return false; }
    public static bool Raycast(Ray ray, float maxDistance, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => false;
    public static bool Raycast(Ray ray, float maxDistance, int layerMask) => false;
    public static bool Raycast(Ray ray, float maxDistance) => false;
    public static bool Raycast(Ray ray) => false;
    public static bool Raycast(Ray ray, out RaycastHit hitInfo, float maxDistance, int layerMask, QueryTriggerInteraction queryTriggerInteraction) { hitInfo = default; return false; }
    public static bool Raycast(Ray ray, out RaycastHit hitInfo, float maxDistance, int layerMask) { hitInfo = default; return false; }
    public static bool Raycast(Ray ray, out RaycastHit hitInfo, float maxDistance) { hitInfo = default; return false; }
    public static bool Raycast(Ray ray, out RaycastHit hitInfo) { hitInfo = default; return false; }
    public static bool Linecast(Vector3 start, Vector3 end, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => false;
    public static bool Linecast(Vector3 start, Vector3 end, int layerMask) => false;
    public static bool Linecast(Vector3 start, Vector3 end) => false;
    public static bool Linecast(Vector3 start, Vector3 end, out RaycastHit hitInfo, int layerMask, QueryTriggerInteraction queryTriggerInteraction) { hitInfo = default; return false; }
    public static bool Linecast(Vector3 start, Vector3 end, out RaycastHit hitInfo, int layerMask) { hitInfo = default; return false; }
    public static bool Linecast(Vector3 start, Vector3 end, out RaycastHit hitInfo) { hitInfo = default; return false; }
    public static RaycastHit[] RaycastAll(Vector3 origin, Vector3 direction, float maxDistance, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => [];
    public static RaycastHit[] RaycastAll(Vector3 origin, Vector3 direction, float maxDistance, int layerMask) => [];
    public static RaycastHit[] RaycastAll(Vector3 origin, Vector3 direction, float maxDistance) => [];
    public static RaycastHit[] RaycastAll(Vector3 origin, Vector3 direction) => [];
    public static RaycastHit[] RaycastAll(Ray ray, float maxDistance, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => [];
    public static RaycastHit[] RaycastAll(Ray ray, float maxDistance, int layerMask) => [];
    public static RaycastHit[] RaycastAll(Ray ray, float maxDistance) => [];
    public static RaycastHit[] RaycastAll(Ray ray) => [];
    public static int RaycastNonAlloc(Ray ray, RaycastHit[] results, float maxDistance, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => 0;
    public static int RaycastNonAlloc(Ray ray, RaycastHit[] results, float maxDistance, int layerMask) => 0;
    public static int RaycastNonAlloc(Ray ray, RaycastHit[] results, float maxDistance) => 0;
    public static int RaycastNonAlloc(Ray ray, RaycastHit[] results) => 0;
    public static int RaycastNonAlloc(Vector3 origin, Vector3 direction, RaycastHit[] results, float maxDistance, int layerMask, QueryTriggerInteraction queryTriggerInteraction) => 0;
    public static int RaycastNonAlloc(Vector3 origin, Vector3 direction, RaycastHit[] results, float maxDistance, int layerMask) => 0;
    public static int RaycastNonAlloc(Vector3 origin, Vector3 direction, RaycastHit[] results, float maxDistance) => 0;
    public static int RaycastNonAlloc(Vector3 origin, Vector3 direction, RaycastHit[] results) => 0;
}
