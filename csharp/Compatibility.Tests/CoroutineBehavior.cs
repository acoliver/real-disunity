using System.Collections;
using CompatibilityBridge;
using UnityEngine;

internal static class CoroutineBehavior
{
    public static void Run()
    {
        var actor = new GameObject("coroutine"); var witness = new GameObject("coroutine-witness");
        using var payload = TestGraph.Payload([new("routine", actor, "CoroutineProbe", new { witness = TestGraph.Ref("witness") })], new() { ["witness"] = witness });
        using var session = new ScriptSession(payload.RootElement); session.Initialize();
        var script = (MonoBehaviour)session.Resolve("routine");
        var routine = (IEnumerator)script.GetType().GetMethod("Wait")!.Invoke(script, null)!;
        var handle = script.StartCoroutine(routine);
        BinderBehavior.Check(witness.transform.position.x == 1, "StartCoroutine moves original enumerator to its first yield");
        Time.timeScale = 0;
        TestGraph.Advance(session, 0.5f);
        BinderBehavior.Check(witness.transform.position.x == 1, "scaled WaitForSeconds stays paused on native clock");
        Time.timeScale = 1;
        script.enabled = false;
        TestGraph.Advance(session, 0.1f);
        BinderBehavior.Check(witness.transform.position.x == 2, "component disable preserves coroutine and nested iterator advances");
        TestGraph.Advance(session, 0.05f);
        BinderBehavior.Check(witness.transform.position.x == 2, "nested wait does not resume early");
        TestGraph.Advance(session, 0.05f);
        BinderBehavior.Check(witness.transform.position.x == 3, "nested wait resumes from original iterator on native time");
        session.Dispatch("Update");
        BinderBehavior.Check(!handle.Completed, "yield null cannot resume twice in the same native frame");
        TestGraph.Advance(session, 0.01f);
        BinderBehavior.Check(handle.Completed && witness.transform.position.y == 1, "iterator finishes and runs finally exactly once");
        var second = (IEnumerator)script.GetType().GetMethod("Wait")!.Invoke(script, null)!;
        script.StartCoroutine(second); script.StopCoroutine(second);
        BinderBehavior.Check(session.CoroutineCount == 0 && witness.transform.position.y == 2, "StopCoroutine matches original enumerator identity and disposes");
        script.StartCoroutine((IEnumerator)script.GetType().GetMethod("Wait")!.Invoke(script, null)!);
        actor.SetActive(false);
        BinderBehavior.Check(session.CoroutineCount == 0 && witness.transform.position.y == 3, "native object deactivation cancels enumerators and disposes finally");
        Console.WriteLine("PASS coroutines: immediate/nested enumeration, native scaled pause, next-frame wait, disabled component, identity cancellation and disposal");
    }
}
public class CoroutineProbe : MonoBehaviour
{
    public GameObject witness = null!;
    public IEnumerator Wait()
    {
        try
        {
            SetX(1); yield return new WaitForSeconds(0.1f);
            yield return Nested();
            SetX(3); yield return null;
        }
        finally { var p = witness.transform.position; p.y++; witness.transform.position = p; }
    }
    private IEnumerator Nested() { SetX(2); yield return new WaitForSeconds(0.1f); }
    private void SetX(float x) { var p = witness.transform.position; p.x = x; witness.transform.position = p; }
}
