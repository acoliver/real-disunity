using Unity.Jobs;
using Unity.Jobs.LowLevel.Unsafe;

namespace UnityEngine.Jobs;

/// <summary>A job that visits each transform of a <see cref="TransformAccessArray"/>.</summary>
[JobProducerType(typeof(IJobParallelForTransformExtensions))]
public interface IJobParallelForTransform
{
    void Execute(int index, TransformAccess transform);
}

/// <summary>
/// One transform of a <see cref="TransformAccessArray"/> as a transform job sees it. It reads and writes the transform
/// directly, because the engine runs a job to completion on the thread that schedules it. A default value is not valid and
/// every member except <see cref="isValid"/> throws <see cref="InvalidOperationException"/>.
/// </summary>
public readonly struct TransformAccess
{
    private readonly Transform? _transform;

    internal TransformAccess(Transform transform) => _transform = transform;

    public bool isValid => _transform is not null && _transform;

    public Vector3 position { get => Live().position; set => Live().position = value; }
    public Quaternion rotation { get => Live().rotation; set => Live().rotation = value; }
    public Vector3 localPosition { get => Live().localPosition; set => Live().localPosition = value; }
    public Quaternion localRotation { get => Live().localRotation; set => Live().localRotation = value; }
    public Vector3 localScale { get => Live().localScale; set => Live().localScale = value; }
    public Matrix4x4 localToWorldMatrix => Live().localToWorldMatrix;
    public Matrix4x4 worldToLocalMatrix => Live().worldToLocalMatrix;

    public void SetPositionAndRotation(Vector3 position, Quaternion rotation)
    {
        var transform = Live();
        transform.position = position;
        transform.rotation = rotation;
    }

    public void GetPositionAndRotation(out Vector3 position, out Quaternion rotation)
    {
        var transform = Live();
        position = transform.position;
        rotation = transform.rotation;
    }

    private Transform Live() =>
        isValid ? _transform! : throw new InvalidOperationException("The TransformAccess is not valid: its transform was destroyed or the value is a default one");
}

/// <summary>Schedules a transform job. The engine runs it at once on the calling thread, on a copy of the job, over the array's
/// transforms in order, and returns a completed handle. A transform destroyed before the job reaches it is passed as an
/// invalid <see cref="TransformAccess"/>.</summary>
public static class IJobParallelForTransformExtensions
{
    public static void EarlyJobInit<T>() where T : struct, IJobParallelForTransform { }

    public static JobHandle Schedule<T>(this T jobData, TransformAccessArray transforms, JobHandle dependsOn = default)
        where T : struct, IJobParallelForTransform => RunByRef(ref jobData, transforms);

    public static JobHandle ScheduleByRef<T>(ref this T jobData, TransformAccessArray transforms, JobHandle dependsOn = default)
        where T : struct, IJobParallelForTransform => RunByRef(ref jobData, transforms);

    public static void Run<T>(this T jobData, TransformAccessArray transforms) where T : struct, IJobParallelForTransform =>
        RunByRef(ref jobData, transforms);

    public static JobHandle RunByRef<T>(ref this T jobData, TransformAccessArray transforms) where T : struct, IJobParallelForTransform
    {
        var copy = jobData;
        for (var index = 0; index < transforms.length; index++) copy.Execute(index, new TransformAccess(transforms[index]));
        return default;
    }
}
