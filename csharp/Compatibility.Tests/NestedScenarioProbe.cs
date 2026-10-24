using System.Collections;
using UnityEngine;

public sealed class NestedScenarioProbe : MonoBehaviour
{
    public int mode, depth = 1, moves, currents, disposals, rejected, after, sentinelMoves;
    public Coroutine? root;
    public string trace = "";
    private void Start()
    {
        if (mode is 30 or 31) root = StartCoroutine(Joining());
        else if (mode == 50) Exhaust();
        else
        {
            var child = StartCoroutine(new Node(this, 1));
            if (mode == 40) { StopCoroutine(child); StartCoroutine(new Node(this, 1)); }
        }
        after++; trace += "A";
    }
    private void Exhaust()
    {
        for (var i = 0; i < 5000; i++)
        {
            try { StartCoroutine(new Node(this, 1)); }
            catch (NativeWorldException error) when (error.Code == "invalid_request") { rejected++; break; }
        }
    }
    private IEnumerator Joining()
    {
        yield return null;
        if (mode == 30) { StartCoroutine(JoinAncestor()); yield return null; }
        else yield return root;
    }
    private IEnumerator JoinAncestor() { try { yield return root; } finally { disposals++; } }
    private void RejectStart()
    {
        try { StartCoroutine(new Sentinel(this)); }
        catch (NativeWorldException error) when (error.Code == "invalid_request") { rejected++; }
    }
    private sealed class Sentinel(NestedScenarioProbe owner) : IEnumerator
    {
        public bool MoveNext() { owner.sentinelMoves++; return false; }
        public object? Current => null;
        public void Reset() => throw new NotSupportedException();
    }
    private sealed class Node(NestedScenarioProbe owner, int level) : IEnumerator, IDisposable
    {
        private int _moves;
        public bool MoveNext()
        {
            owner.moves++; _moves++; owner.trace += "M" + level;
            if (owner.mode == 10) throw new ArgumentException("nested sentinel MoveNext");
            if (owner.mode == 0 && level < owner.depth)
            {
                try { owner.StartCoroutine(new Node(owner, level + 1)); }
                catch (NativeWorldException error) when (error.Code == "invalid_request") { owner.rejected++; }
            }
            return owner.mode is not (0 or 12 or 50) && _moves == 1;
        }
        public object? Current
        {
            get
            {
                owner.currents++;
                if (owner.mode == 11) throw new ArgumentException("nested sentinel Current");
                if (owner.mode == 1) owner.RejectStart();
                if (owner.mode == 20) owner.gameObject.SetActive(false);
                if (owner.mode == 21) UnityEngine.Object.Destroy(owner.gameObject);
                if (owner.mode == 22) owner.StopAllCoroutines();
                return null;
            }
        }
        public void Dispose()
        {
            owner.disposals++; owner.trace += "D" + level;
            if (owner.mode == 1) owner.RejectStart();
            if (owner.mode == 12) throw new ArgumentException("nested sentinel Dispose");
        }
        public void Reset() => throw new NotSupportedException();
    }
}
