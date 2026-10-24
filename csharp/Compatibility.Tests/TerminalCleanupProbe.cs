using System.Collections;
using UnityEngine;

public sealed class TerminalCleanupProbe : MonoBehaviour
{
    public bool failCleanup;
    private void FixedUpdate()
    {
        var p = transform.position; p.x++; transform.position = p;
        var scale = transform.localScale; scale.y = UnityEngine.Random.Range(0, 1000000); transform.localScale = scale;
        _ = Input.GetKeyDown(KeyCode.W);
    }
    private void Update() { var p = transform.localScale; p.x++; transform.localScale = p; }
    private IEnumerator Start()
    {
        try { yield return new WaitForSeconds(100); }
        finally
        {
            var p = transform.position; p.y++; transform.position = p;
            if (failCleanup) throw new ArgumentException("terminal Dispose sentinel");
        }
    }
    private void OnDisable()
    {
        var p = transform.position; p.y += 10; transform.position = p;
        var scale = transform.localScale; scale.z = UnityEngine.Random.Range(0, 1000000); transform.localScale = scale;
        if (failCleanup) throw new ArgumentException("terminal OnDisable sentinel");
    }
    private void OnDestroy()
    {
        var p = transform.position; p.z += 10; transform.position = p;
        if (failCleanup) throw new ArgumentException("terminal OnDestroy sentinel");
    }
}
