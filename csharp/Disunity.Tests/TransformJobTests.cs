using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Jobs;

/// <summary>An IJobParallelForTransform visits the transforms of an array in order, reads and writes them directly and gives an
/// invalid access for a destroyed transform.</summary>
internal static class TransformJobTests
{
    private struct Lift : IJobParallelForTransform
    {
        public float Height;

        public void Execute(int index, TransformAccess transform)
        {
            var p = transform.position;
            transform.position = new Vector3(p.x, Height + index, p.z);
        }
    }

    private struct Probe : IJobParallelForTransform
    {
        public List<(int Index, bool Valid)> Seen;

        public void Execute(int index, TransformAccess transform) => Seen.Add((index, transform.isValid));
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var objects = Enumerable.Range(0, 3).Select(i => DisunityHost.CreateGameObject()).ToArray();
            foreach (var (obj, i) in objects.Select((o, i) => (o, i))) obj.transform.position = new Vector3(i, 0, 5);
            var array = new TransformAccessArray(objects.Select(o => o.transform).ToArray());
            var handle = new Lift { Height = 10f }.Schedule(array);
            Expect.That(handle.IsCompleted, "the job is done when Schedule returns");
            Expect.Sequence(new[] { 10f, 11f, 12f }, objects.Select(o => o.transform.position.y), "each transform moved by its index");
            Expect.Sequence(new[] { 0f, 1f, 2f }, objects.Select(o => o.transform.position.x), "the other axes are kept");
            var lift = new Lift { Height = 0f };
            lift.Run(array);
            lift.ScheduleByRef(array);
            Expect.Sequence(new[] { 0f, 1f, 2f }, objects.Select(o => o.transform.position.y), "Run and ScheduleByRef visit the same transforms");
            var seen = new List<(int, bool)>();
            UnityEngine.Object.DestroyImmediate(objects[1]);
            new Probe { Seen = seen }.Run(array);
            Expect.That(seen.Count == 3 && seen[0] == (0, true) && seen[1] == (1, false) && seen[2] == (2, true), $"a destroyed transform is an invalid access: {string.Join(",", seen)}");
            Expect.Throws<InvalidOperationException>(() => _ = default(TransformAccess).position, "a default access throws");
            Expect.That(!default(TransformAccess).isValid, "a default access is not valid");
            array.Dispose();
            Expect.Throws<InvalidOperationException>(() => new Lift().Run(array), "a disposed array");
            IJobParallelForTransformExtensions.EarlyJobInit<Lift>();
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
        Console.WriteLine("PASS: an IJobParallelForTransform visits the array's transforms in order and sees a destroyed one as invalid");
    }
}
