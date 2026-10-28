using System.Collections.Concurrent;
using System.Reflection;
using System.Runtime.CompilerServices;
using Unity.Collections;
using Unity.Jobs.LowLevel.Unsafe;

namespace Unity.Jobs
{
    [JobProducerType(typeof(IJobExtensions))]
    public interface IJob
    {
        void Execute();
    }

    [JobProducerType(typeof(IJobParallelForExtensions))]
    public interface IJobParallelFor
    {
        void Execute(int index);
    }

    [JobProducerType(typeof(IJobForExtensions))]
    public interface IJobFor
    {
        void Execute(int index);
    }

    /// <summary>
    /// A scheduled job. Jobs run to completion on the thread that schedules them, after the
    /// jobs they depend on, so every handle is complete. That is one of the orders Unity can
    /// run them in: code may not use a job's containers between scheduling it and completing it.
    /// </summary>
    public struct JobHandle : IEquatable<JobHandle>
    {
        public readonly bool IsCompleted => true;
        public readonly void Complete() { }
        public static void CompleteAll(ref JobHandle job0, ref JobHandle job1) { }
        public static void CompleteAll(ref JobHandle job0, ref JobHandle job1, ref JobHandle job2) { }
        public static void CompleteAll(NativeArray<JobHandle> jobs) { }
        public static void ScheduleBatchedJobs() { }
        public static JobHandle CombineDependencies(JobHandle job0, JobHandle job1) => default;
        public static JobHandle CombineDependencies(JobHandle job0, JobHandle job1, JobHandle job2) => default;
        public static JobHandle CombineDependencies(NativeArray<JobHandle> jobs) => default;
        public static JobHandle CombineDependencies(NativeSlice<JobHandle> jobs) => default;
        public static bool CheckFenceIsDependencyOrDidSyncFence(JobHandle jobHandle, JobHandle dependsOn) => true;
        // Every handle is a completed job, so all handles are equal.
        public readonly bool Equals(JobHandle other) => true;
        public override readonly bool Equals(object? obj) => obj is JobHandle;
        public override readonly int GetHashCode() => 0;
        public static bool operator ==(JobHandle a, JobHandle b) => a.Equals(b);
        public static bool operator !=(JobHandle a, JobHandle b) => !a.Equals(b);
    }

    public static class IJobExtensions
    {
        public static void EarlyJobInit<T>() where T : struct, IJob { }
        public static JobHandle Schedule<T>(this T jobData, JobHandle dependsOn = default) where T : struct, IJob => RunByRef(ref jobData);
        public static JobHandle ScheduleByRef<T>(ref this T jobData, JobHandle dependsOn = default) where T : struct, IJob => RunByRef(ref jobData);
        public static void Run<T>(this T jobData) where T : struct, IJob => RunByRef(ref jobData);

        /// <summary>Runs a copy of the job, as Unity runs a job on its own copy of the data.</summary>
        public static JobHandle RunByRef<T>(ref this T jobData) where T : struct, IJob
        {
            var copy = jobData;
            copy.Execute();
            return default;
        }
    }

    public static class IJobParallelForExtensions
    {
        public static void EarlyJobInit<T>() where T : struct, IJobParallelFor { }

        public static JobHandle Schedule<T>(this T jobData, int arrayLength, int innerloopBatchCount, JobHandle dependsOn = default)
            where T : struct, IJobParallelFor => RunByRef(ref jobData, arrayLength);

        public static JobHandle ScheduleByRef<T>(ref this T jobData, int arrayLength, int innerloopBatchCount, JobHandle dependsOn = default)
            where T : struct, IJobParallelFor => RunByRef(ref jobData, arrayLength);

        public static void Run<T>(this T jobData, int arrayLength) where T : struct, IJobParallelFor => RunByRef(ref jobData, arrayLength);

        public static JobHandle RunByRef<T>(ref this T jobData, int arrayLength) where T : struct, IJobParallelFor
        {
            var copy = jobData;
            for (var i = 0; i < arrayLength; i++) copy.Execute(i);
            return default;
        }
    }

    public static class IJobForExtensions
    {
        public static void EarlyJobInit<T>() where T : struct, IJobFor { }
        public static void Run<T>(this T jobData, int arrayLength) where T : struct, IJobFor => RunByRef(ref jobData, arrayLength);

        public static JobHandle Schedule<T>(this T jobData, int arrayLength, JobHandle dependency) where T : struct, IJobFor =>
            RunByRef(ref jobData, arrayLength);

        public static JobHandle ScheduleParallel<T>(this T jobData, int arrayLength, int innerloopBatchCount, JobHandle dependency)
            where T : struct, IJobFor => RunByRef(ref jobData, arrayLength);

        public static JobHandle ScheduleByRef<T>(ref this T jobData, int arrayLength, JobHandle dependency) where T : struct, IJobFor =>
            RunByRef(ref jobData, arrayLength);

        public static JobHandle ScheduleParallelByRef<T>(ref this T jobData, int arrayLength, int innerloopBatchCount, JobHandle dependency)
            where T : struct, IJobFor => RunByRef(ref jobData, arrayLength);

        public static JobHandle RunByRef<T>(ref this T jobData, int arrayLength) where T : struct, IJobFor
        {
            var copy = jobData;
            for (var i = 0; i < arrayLength; i++) copy.Execute(i);
            return default;
        }
    }
}

namespace Unity.Jobs.LowLevel.Unsafe
{
    public enum JobType
    {
        Single = 0,
        ParallelFor = 1,
    }

    public enum ScheduleMode
    {
        Run = 0,
        Batched = 1,
        Parallel = 1,
        Single = 2,
    }

    [AttributeUsage(AttributeTargets.Interface)]
    public sealed class JobProducerTypeAttribute(Type producerType) : Attribute
    {
        public Type ProducerType { get; } = producerType;
    }

    /// <summary>The iterations a job runs. <c>Next</c> is the first iteration not yet handed out.</summary>
    public struct JobRanges
    {
        internal int BatchSize;
        internal int NumJobs;
        public int TotalIterationCount;
        internal int Next;
    }

    /// <summary>
    /// The job system's low-level entry points for job producers. A producer registers the
    /// delegate that runs its jobs, Unity's <c>ExecuteJobFunction(ref T data, IntPtr
    /// additionalPtr, IntPtr bufferRangePatchData, ref JobRanges ranges, int jobIndex)</c>, and
    /// schedules a pointer to the job's data. Jobs run on the scheduling thread as one job
    /// that takes every batch of iterations; there are no worker threads.
    /// </summary>
    public static unsafe class JobsUtility
    {
        public const int MaxJobThreadCount = 128;
        public const int CacheLineSize = 64;

        public struct JobScheduleParameters
        {
            public JobHandle Dependency;
            public int ScheduleMode;
            public IntPtr ReflectionData;
            public IntPtr JobDataPtr;

            public JobScheduleParameters(void* i_jobData, IntPtr i_reflectionData, JobHandle i_dependency, ScheduleMode i_scheduleMode)
            {
                Dependency = i_dependency;
                JobDataPtr = (IntPtr)i_jobData;
                ReflectionData = i_reflectionData;
                ScheduleMode = (int)i_scheduleMode;
            }
        }

        private static readonly ConcurrentDictionary<IntPtr, Delegate> Reflection = new();
        private static readonly ConcurrentDictionary<Type, MethodInfo> Readers = new();
        private static long _nextReflection;

        public static bool IsExecutingJob => false;
        public static bool JobDebuggerEnabled { get; set; }
        public static bool JobCompilerEnabled { get; set; } = true;
        public static int JobWorkerMaximumCount => 0;
        public static int JobWorkerCount { get => 0; set { } }
        public static int ThreadIndex => 0;
        public static int ThreadIndexCount => 1;
        public static void ResetJobWorkerCount() { }

        public static IntPtr CreateJobReflectionData(Type type, JobType jobType, object managedJobFunction0, object? managedJobFunction1 = null, object? managedJobFunction2 = null) =>
            Register(managedJobFunction0);

        public static IntPtr CreateJobReflectionData(Type type, object managedJobFunction0, object? managedJobFunction1 = null, object? managedJobFunction2 = null) =>
            Register(managedJobFunction0);

        public static IntPtr CreateJobReflectionData(Type wrapperJobType, Type userJobType, JobType jobType, object managedJobFunction0) =>
            Register(managedJobFunction0);

        public static IntPtr CreateJobReflectionData(Type wrapperJobType, Type userJobType, object managedJobFunction0) =>
            Register(managedJobFunction0);

        private static IntPtr Register(object function)
        {
            var execute = function as Delegate ?? throw new ArgumentException("a job's execute function must be a delegate", nameof(function));
            var id = (IntPtr)Interlocked.Increment(ref _nextReflection);
            Reflection[id] = execute;
            return id;
        }

        public static JobHandle Schedule(ref JobScheduleParameters parameters) => Execute(parameters, 1, 1);

        public static JobHandle ScheduleParallelFor(ref JobScheduleParameters parameters, int arrayLength, int innerloopBatchCount) =>
            Execute(parameters, arrayLength, innerloopBatchCount);

        /// <summary>A parallel-for over a list's length when the job runs: <paramref name="listData"/>
        /// points at the list's <c>UnsafeList</c> pointer, whose length follows its buffer pointer.</summary>
        public static JobHandle ScheduleParallelForDeferArraySize(ref JobScheduleParameters parameters, int innerloopBatchCount, void* listData, void* listDataAtomicSafetyHandle)
        {
            var list = *(byte**)listData;
            var length = *(int*)(list + sizeof(void*));
            return Execute(parameters, length, innerloopBatchCount);
        }

        public static bool GetWorkStealingRange(ref JobRanges ranges, int jobIndex, out int beginIndex, out int endIndex)
        {
            beginIndex = ranges.Next;
            endIndex = Math.Min(ranges.TotalIterationCount, beginIndex + Math.Max(ranges.BatchSize, 1));
            if (beginIndex >= ranges.TotalIterationCount) return false;
            ranges.Next = endIndex;
            return true;
        }

        public static void GetJobRange(ref JobRanges ranges, int jobIndex, out int beginIndex, out int endIndex)
        {
            beginIndex = 0;
            endIndex = ranges.TotalIterationCount;
        }

        private static JobHandle Execute(JobScheduleParameters parameters, int length, int batchSize)
        {
            if (!Reflection.TryGetValue(parameters.ReflectionData, out var execute))
                throw new InvalidOperationException("the job's reflection data was not created by JobsUtility.CreateJobReflectionData");
            var data = execute.Method.GetParameters()[0].ParameterType.GetElementType()!;
            var reader = Readers.GetOrAdd(data, type => typeof(JobsUtility).GetMethod(nameof(ReadJob), BindingFlags.NonPublic | BindingFlags.Static)!
                .MakeGenericMethod(type));
            var job = reader.Invoke(null, [parameters.JobDataPtr]);
            var ranges = new JobRanges { BatchSize = batchSize, NumJobs = 1, TotalIterationCount = length };
            try { execute.DynamicInvoke(job, IntPtr.Zero, IntPtr.Zero, ranges, 0); }
            catch (TargetInvocationException error) when (error.InnerException is { } inner)
            {
                System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(inner).Throw();
            }
            return default;
        }

        /// <summary>A copy of the job's data, which the job runs on, as Unity copies it when scheduling.</summary>
        private static object ReadJob<T>(IntPtr data) => System.Runtime.CompilerServices.Unsafe.Read<T>((void*)data)!;
    }
}

namespace Unity.Burst.LowLevel
{
    /// <summary>
    /// The native side of the Burst compiler. A player ships Burst-compiled code as a native
    /// library for its platform; the engine loads none, so Burst reports itself uninitialized
    /// and runs Burst functions and jobs as managed code.
    /// </summary>
    public static unsafe class BurstCompilerService
    {
        public enum BurstLogType
        {
            Info = 0,
            Warning = 1,
            Error = 2,
        }

        private static uint _executionMode;

        public static bool IsInitialized => false;
        public static bool LoadBurstLibrary(string fullPathToLibBurstGenerated) => false;
        public static void SetCurrentExecutionMode(uint environment) => _executionMode = environment;
        public static uint GetCurrentExecutionMode() => _executionMode;
        public static string GetMethodSignature(MethodInfo method) => method.ToString() ?? "";
        public static string GetDisassembly(MethodInfo m, string compilerOptions) => "";
        public static int CompileAsyncDelegateMethod(object delegateMethod, string compilerOptions) => -1;
        public static void* GetAsyncCompiledAsyncDelegateMethod(int userID) => null;

        public static void Log(void* userData, BurstLogType logType, byte* message, byte* filename, int lineNumber) =>
            Write(logType, message);

        public static void RuntimeLog(void* userData, BurstLogType logType, byte* message, byte* filename, int lineNumber) =>
            Write(logType, message);

        private static void Write(BurstLogType logType, byte* message)
        {
            var text = System.Runtime.InteropServices.Marshal.PtrToStringUTF8((IntPtr)message) ?? "";
            switch (logType)
            {
                case BurstLogType.Error: UnityEngine.Debug.LogError(text); break;
                case BurstLogType.Warning: UnityEngine.Debug.LogWarning(text); break;
                default: UnityEngine.Debug.Log(text); break;
            }
        }
    }
}
