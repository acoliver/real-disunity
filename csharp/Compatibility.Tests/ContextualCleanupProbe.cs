using UnityEngine;
using UObject = UnityEngine.Object;

public sealed class ContextualCleanupProbe : MonoBehaviour
{
    public int role;
    public MonoBehaviour? sibling;
    public MonoBehaviour? removed;
    public int disables;
    public int destroys;
    public static int Initialized;
    public static int Entries;
    public static int Throws;
    public static int Mutations;
    public static bool AttemptRoutine;
    public static bool RoutineRejected;
    private void Awake() => Initialized++;
    private void OnEnable() => Initialized++;
    private void OnDisable()
    {
        disables++;
        if (role == 0 && sibling is not null && removed is not null)
        {
            sibling.enabled = false;
            UObject.Destroy(removed);
            Mutations++;
        }
        if (AttemptRoutine)
        {
            try { StartCoroutine(Empty()); }
            catch (NotSupportedException) { RoutineRejected = true; }
        }
        Enter();
    }
    private void OnDestroy() { destroys++; Enter(); }
    private static System.Collections.IEnumerator Empty() { yield break; }
    private static void Enter()
    {
        var entry = Entries++;
        if (entry % 97 != 0) return;
        Throws++;
        throw new ArgumentException("contextual Cleanup actual CLR failure " + entry);
    }
}
