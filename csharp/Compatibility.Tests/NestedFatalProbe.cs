using System.Collections;
using UnityEngine;

public sealed class NestedFatalProbe : MonoBehaviour
{
    private IEnumerator Start()
    {
        var p = transform.position; p.x++; transform.position = p;
        StartCoroutine(Child());
        p = transform.position; p.x += 10; transform.position = p;
        try { yield return new WaitForSeconds(100); }
        finally { p = transform.position; p.y++; transform.position = p; }
    }
    private IEnumerator Child()
    {
        var p = transform.position; p.z++; transform.position = p;
        try { yield return new WaitForSeconds(100); }
        finally { p = transform.position; p.y++; transform.position = p; throw new ArgumentException("nested fatal child Dispose sentinel"); }
    }
}
