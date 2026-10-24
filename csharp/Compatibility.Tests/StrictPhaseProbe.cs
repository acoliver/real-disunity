using System.Collections;
using UnityEngine;

public sealed class StrictPhaseProbe : MonoBehaviour
{
    public bool failUpdate;
    public bool failCleanup;
    public int updates;
    public int lateUpdates;
    public int fixedUpdates;
    public int fixedResumes;
    public int updateResumes;
    public int endResumes;
    public bool fixedHeld;
    public bool fixedDown;
    public bool updateHeld;
    public float fixedDelta;
    public float dynamicDelta;
    public float unscaled;
    public bool settersRejected;
    public bool bypassRejected;
    private void Awake() { var p = transform.position; p.x++; transform.position = p; }
    private void Start()
    {
        StartCoroutine(FixedRoutine());
        StartCoroutine(UpdateRoutine());
        StartCoroutine(EndRoutine());
        StartCoroutine(CleanupRoutine());
    }
    private IEnumerator FixedRoutine() { yield return new WaitForFixedUpdate(); fixedResumes++; }
    private IEnumerator UpdateRoutine() { yield return null; updateResumes++; }
    private IEnumerator EndRoutine() { yield return new WaitForEndOfFrame(); endResumes++; }
    private IEnumerator CleanupRoutine()
    {
        try { yield return new WaitForSeconds(100); }
        finally
        {
            var p = transform.position; p.x += 10; transform.position = p;
            if (failCleanup) throw new ArgumentException("strict Dispose sentinel");
        }
    }
    private void OnDisable()
    {
        var p = transform.position; p.y += 10; transform.position = p;
        if (failCleanup) throw new ArgumentException("strict OnDisable sentinel");
    }
    private void OnDestroy()
    {
        var p = transform.position; p.z += 10; transform.position = p;
        if (failCleanup) throw new ArgumentException("strict OnDestroy sentinel");
    }
    private void FixedUpdate()
    {
        fixedUpdates++;
        fixedHeld = Input.GetKey(KeyCode.W);
        fixedDown = Input.GetKeyDown(KeyCode.W);
        fixedDelta = Time.deltaTime;
        unscaled = Time.fixedUnscaledDeltaTime;
    }
    private void Update()
    {
        updates++;
        updateHeld = Input.GetKey(KeyCode.W);
        dynamicDelta = Time.unscaledDeltaTime;
        try { Time.timeScale = 0; }
        catch (NotSupportedException) { settersRejected = true; }
        try { ((CompatibilityBridge.ScriptSession)NativeIdentity.Runtime!).Dispatch("Update"); }
        catch (InvalidOperationException) { bypassRejected = true; }
        if (failUpdate) throw new ArgumentException("strict Update sentinel");
    }
    private void LateUpdate() => lateUpdates++;
}
