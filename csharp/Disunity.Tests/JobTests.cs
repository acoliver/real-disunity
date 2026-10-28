using System.Reflection;
using Unity.Burst.LowLevel;
using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;
using Unity.Jobs;
using Unity.Jobs.LowLevel.Unsafe;

/// <summary>
/// Unity's job system as the engine runs it: a job executes on the scheduling thread before
/// the schedule call returns, so every handle is complete and the job's containers hold its
/// results at once.
/// </summary>
internal static unsafe class JobTests
{
    public static void Run()
    {
        SingleJobsRunOnceBeforeScheduleReturns();
        JobsRunOnACopyOfTheirData();
        ParallelJobsVisitEveryIndexOnce();
        IndexedJobsRunInOrderWhenRunOrScheduled();
        ParallelIndexedJobsVisitEveryIndexOnce();
        DependenciesRunFirstAndHandlesAreComplete();
        JobProducersAreFoundThroughTheirInterfaces();
        WorkStealingHandsOutEveryBatchOnce();
        RegisteredJobsRunThroughJobsUtility();
        JobsUtilityReportsFailuresAsTheyHappened();
        DeferredArraySizeReadsTheListLength();
        BurstReportsItselfUninitialized();
        Console.WriteLine("PASS: jobs run synchronously with complete handles, every index once, and low-level job registration");
    }

    private static void SingleJobsRunOnceBeforeScheduleReturns()
    {
        var data = new NativeArray<int>(2, Allocator.TempJob);
        var handle = new WriteJob { Data = data }.Schedule();
        Expect.That(data[0] == 1 && data[1] == 42, "Execute ran once, and its writes are visible, before Schedule returned");
        Expect.That(handle.IsCompleted, "the handle is complete");
        handle.Complete();
        Expect.Equal(1, data[0], "Complete does not run the job again");
        new WriteJob { Data = data }.Run();
        Expect.Equal(2, data[0], "Run executes the job once");
        var job = new WriteJob { Data = data };
        job.ScheduleByRef();
        job.RunByRef();
        Expect.Equal(4, data[0], "ScheduleByRef and RunByRef execute once each");
        Expect.Throws<InvalidOperationException>(() => new FailingJob().Schedule(), "an exception in Execute reaches the scheduling code");
        IJobExtensions.EarlyJobInit<WriteJob>();
        Expect.Equal(4, data[0], "EarlyJobInit schedules nothing");
        data.Dispose();
    }

    private static void JobsRunOnACopyOfTheirData()
    {
        var data = new NativeArray<int>(2, Allocator.TempJob);
        var job = new WriteJob { Data = data };
        job.ScheduleByRef();
        Expect.Equal(0, job.Local, "the scheduling code's job struct is not changed by the job, as Unity runs jobs on a copy");
        job.RunByRef();
        Expect.Equal(0, job.Local, "RunByRef also runs on a copy");
        Expect.Equal(2, data[0], "both runs executed");
        data.Dispose();
    }

    private static void ParallelJobsVisitEveryIndexOnce()
    {
        foreach (var (length, batch) in new[] { (0, 1), (1, 1), (10, 3), (10, 10), (10, 64), (97, 7), (64, 8), (5, 1) })
        {
            var hits = new NativeArray<int>(length, Allocator.TempJob);
            var handle = new HitJob { Hits = hits }.Schedule(length, batch);
            Expect.That(handle.IsCompleted && hits.ToArray().All(count => count == 1), $"IJobParallelFor length {length} batch {batch} visits each index once");
            new HitJob { Hits = hits }.Run(length);
            Expect.That(hits.ToArray().All(count => count == 2), $"Run visits each of {length} indexes once");
            var byRef = new HitJob { Hits = hits };
            byRef.ScheduleByRef(length, batch);
            byRef.RunByRef(length);
            Expect.That(hits.ToArray().All(count => count == 4), $"ScheduleByRef and RunByRef visit each of {length} indexes once");
            hits.Dispose();
        }
    }

    private static void IndexedJobsRunInOrderWhenRunOrScheduled()
    {
        var order = new NativeArray<int>(5, Allocator.TempJob);
        var cursor = new NativeArray<int>(1, Allocator.TempJob);
        var job = new OrderJob { Order = order, Cursor = cursor };
        job.Run(5);
        Expect.Sequence(new[] { 0, 1, 2, 3, 4 }, order.ToArray(), "IJobFor.Run visits indexes in order");
        cursor[0] = 0;
        var handle = job.Schedule(5, default);
        Expect.That(handle.IsCompleted && cursor[0] == 5, "IJobFor.Schedule has finished when it returns");
        cursor[0] = 0;
        job.ScheduleByRef(5, default);
        job.RunByRef(5);
        Expect.Equal(10, cursor[0], "ScheduleByRef and RunByRef of an IJobFor");
        cursor[0] = 0;
        job.Run(0);
        job.Schedule(0, default);
        Expect.Equal(0, cursor[0], "an empty IJobFor runs nothing");
        IJobForExtensions.EarlyJobInit<OrderJob>();
        order.Dispose();
        cursor.Dispose();
    }

    private static void ParallelIndexedJobsVisitEveryIndexOnce()
    {
        foreach (var (length, batch) in new[] { (0, 4), (9, 2), (9, 9), (16, 5) })
        {
            var hits = new NativeArray<int>(length, Allocator.TempJob);
            var job = new ForHitJob { Hits = hits };
            var handle = job.ScheduleParallel(length, batch, default);
            Expect.That(handle.IsCompleted && hits.ToArray().All(count => count == 1), $"ScheduleParallel length {length} batch {batch}");
            job.ScheduleParallelByRef(length, batch, default);
            Expect.That(hits.ToArray().All(count => count == 2), $"ScheduleParallelByRef length {length} batch {batch}");
            hits.Dispose();
        }
    }

    private static void DependenciesRunFirstAndHandlesAreComplete()
    {
        var data = new NativeArray<int>(2, Allocator.TempJob);
        var first = new SetJob { Data = data, Value = 1 }.Schedule();
        var second = new ScaleJob { Data = data }.Schedule(first);
        Expect.Equal(12, data[0], "a dependent job runs after the job it depends on");
        var hits = new NativeArray<int>(4, Allocator.TempJob);
        var third = new HitJob { Hits = hits }.Schedule(4, 2, second);
        var fourth = new ForHitJob { Hits = hits }.Schedule(4, third);
        Expect.That(hits.ToArray().All(count => count == 2) && fourth.IsCompleted, "parallel and indexed jobs accept dependencies");
        var combined = JobHandle.CombineDependencies(first, second);
        var three = JobHandle.CombineDependencies(first, second, third);
        var handles = new NativeArray<JobHandle>([first, second, third], Allocator.TempJob);
        var fromArray = JobHandle.CombineDependencies(handles);
        var fromSlice = JobHandle.CombineDependencies(new NativeSlice<JobHandle>(handles));
        Expect.That(combined.IsCompleted && three.IsCompleted && fromArray.IsCompleted && fromSlice.IsCompleted, "combined handles are complete");
        combined.Complete();
        JobHandle.CompleteAll(ref first, ref second);
        JobHandle.CompleteAll(ref first, ref second, ref third);
        JobHandle.CompleteAll(handles);
        JobHandle.ScheduleBatchedJobs();
        Expect.That(default(JobHandle).IsCompleted && default(JobHandle) == first && !(first != second), "a default handle is a complete job");
        Expect.That(first.Equals((object)second) && first.GetHashCode() == second.GetHashCode(), "all handles are equal");
        Expect.That(JobHandle.CheckFenceIsDependencyOrDidSyncFence(first, second), "every fence is already synchronized");
        var empty = new NativeArray<JobHandle>(0, Allocator.TempJob);
        Expect.That(JobHandle.CombineDependencies(empty).IsCompleted, "combining no handles gives a complete handle");
        Expect.Equal(12, data[0], "completing handles runs nothing");
        data.Dispose();
        hits.Dispose();
        handles.Dispose();
        empty.Dispose();
    }

    private static void JobProducersAreFoundThroughTheirInterfaces()
    {
        foreach (var (job, jobType) in new[] { (typeof(IJob), typeof(WriteJob)), (typeof(IJobParallelFor), typeof(HitJob)), (typeof(IJobFor), typeof(OrderJob)) })
        {
            var producer = job.GetCustomAttribute<JobProducerTypeAttribute>()!.ProducerType;
            var init = producer.GetMethod("EarlyJobInit")!.MakeGenericMethod(jobType);
            init.Invoke(null, null);
            Expect.That(producer.GetMethod("Schedule") is not null, $"{job.Name}'s producer schedules jobs");
        }
    }

    private static void WorkStealingHandsOutEveryBatchOnce()
    {
        (int Length, int Batch, (int, int)[] Batches)[] cases =
        [
            (10, 3, [(0, 3), (3, 6), (6, 9), (9, 10)]),
            (8, 4, [(0, 4), (4, 8)]),
            (5, 100, [(0, 5)]),
            (3, 1, [(0, 1), (1, 2), (2, 3)]),
            (0, 4, []),
        ];
        var reflection = JobsUtility.CreateJobReflectionData(typeof(RangeData), JobType.ParallelFor, new RangeFunction(ExecuteRanges));
        foreach (var (length, batch, expected) in cases)
        {
            Batches.Clear();
            var hits = new NativeArray<int>(length, Allocator.TempJob);
            var data = new RangeData { Hits = hits };
            var parameters = new JobsUtility.JobScheduleParameters(UnsafeUtility.AddressOf(ref data), reflection, default, ScheduleMode.Parallel);
            var handle = JobsUtility.ScheduleParallelFor(ref parameters, length, batch);
            Expect.That(handle.IsCompleted, "ScheduleParallelFor returns a complete handle");
            Expect.Sequence(expected, Batches, $"batches of length {length} batch {batch}");
            Expect.That(hits.ToArray().All(count => count == 1), $"every index of {length} is covered by exactly one batch");
            hits.Dispose();
        }
    }

    private static void RegisteredJobsRunThroughJobsUtility()
    {
        var data = new NativeArray<int>(2, Allocator.TempJob);
        var job = new WriteJob { Data = data };
        var handle = PlainProducer<WriteJob>.Schedule(ref job);
        Expect.That(handle.IsCompleted && data[0] == 1 && data[1] == 42, "a registered job ran once before JobsUtility.Schedule returned");
        Expect.Equal(0, job.Local, "the registered job ran on a copy of its data");
        PlainProducer<WriteJob>.Schedule(ref job, handle);
        Expect.Equal(2, data[0], "a second schedule runs the job again with the same registration");
        Expect.That(PlainProducer<WriteJob>.Initialize() == PlainProducer<WriteJob>.Initialize(), "registration is reused");
        var unknown = new JobsUtility.JobScheduleParameters(UnsafeUtility.AddressOf(ref job), (IntPtr)0x7777, default, ScheduleMode.Run);
        Expect.Throws<InvalidOperationException>(() => JobsUtility.Schedule(ref unknown), "reflection data that was never registered");
        Expect.Throws<ArgumentException>(() => JobsUtility.CreateJobReflectionData(typeof(WriteJob), JobType.Single, "not a delegate"),
            "an execute function that is not a delegate");
        data.Dispose();
    }

    private static void JobsUtilityReportsFailuresAsTheyHappened()
    {
        var failing = new FailingJob();
        var parameters = new JobsUtility.JobScheduleParameters(UnsafeUtility.AddressOf(ref failing), PlainProducer<FailingJob>.Initialize(), default, ScheduleMode.Run);
        try { JobsUtility.Schedule(ref parameters); }
        catch (Exception error)
        {
            Expect.That(error is InvalidOperationException { Message: "job failed" }, $"the job's own exception is thrown, not a wrapper: {error.GetType().Name}");
            return;
        }
        throw new InvalidOperationException("a job's exception was lost");
    }

    private static void DeferredArraySizeReadsTheListLength()
    {
        var hits = new NativeArray<int>(6, Allocator.TempJob);
        var list = (byte*)UnsafeUtility.Malloc(16, 8, Allocator.Persistent);
        *(void**)list = hits.GetUnsafePtr();
        *(int*)(list + sizeof(void*)) = 6;
        var data = new RangeData { Hits = hits };
        var reflection = JobsUtility.CreateJobReflectionData(typeof(RangeData), JobType.ParallelFor, new RangeFunction(ExecuteRanges));
        var parameters = new JobsUtility.JobScheduleParameters(UnsafeUtility.AddressOf(ref data), reflection, default, ScheduleMode.Parallel);
        Batches.Clear();
        JobsUtility.ScheduleParallelForDeferArraySize(ref parameters, 4, &list, null);
        Expect.Sequence(new[] { (0, 4), (4, 6) }, Batches, "batches over the length stored in the list");
        Expect.That(hits.ToArray().All(count => count == 1), "every element of the list was visited once");
        UnsafeUtility.Free(list, Allocator.Persistent);
        hits.Dispose();
    }

    private static void BurstReportsItselfUninitialized()
    {
        Expect.That(!BurstCompilerService.IsInitialized, "no Burst library is loaded, so Burst code runs as managed code");
        Expect.That(!BurstCompilerService.LoadBurstLibrary("/nowhere/lib_burst_generated.so"), "a Burst library is not loaded");
        BurstCompilerService.SetCurrentExecutionMode(3);
        Expect.Equal(3u, BurstCompilerService.GetCurrentExecutionMode(), "the execution mode is kept");
        Expect.That(!JobsUtility.IsExecutingJob && JobsUtility.ThreadIndexCount == 1 && JobsUtility.JobWorkerCount == 0, "there are no worker threads");
    }

    private static readonly List<(int, int)> Batches = new();

    private struct RangeData
    {
        public NativeArray<int> Hits;
    }

    private delegate void RangeFunction(ref RangeData data, IntPtr additionalPtr, IntPtr bufferRangePatchData, ref JobRanges ranges, int jobIndex);

    private static void ExecuteRanges(ref RangeData data, IntPtr additionalPtr, IntPtr bufferRangePatchData, ref JobRanges ranges, int jobIndex)
    {
        while (JobsUtility.GetWorkStealingRange(ref ranges, jobIndex, out var begin, out var end))
        {
            Batches.Add((begin, end));
            for (var i = begin; i < end; i++) data.Hits[i]++;
        }
    }

    /// <summary>A job producer written the way Unity's own are: it registers a function that runs
    /// the job and schedules a pointer to the job's data.</summary>
    private static class PlainProducer<T> where T : struct, IJob
    {
        private delegate void ExecuteFunction(ref T data, IntPtr additionalPtr, IntPtr bufferRangePatchData, ref JobRanges ranges, int jobIndex);
        private static IntPtr _reflection;

        public static IntPtr Initialize() =>
            _reflection != IntPtr.Zero ? _reflection : _reflection = JobsUtility.CreateJobReflectionData(typeof(T), JobType.Single, new ExecuteFunction(Execute));

        public static JobHandle Schedule(ref T job, JobHandle dependsOn = default)
        {
            var parameters = new JobsUtility.JobScheduleParameters(UnsafeUtility.AddressOf(ref job), Initialize(), dependsOn, ScheduleMode.Single);
            return JobsUtility.Schedule(ref parameters);
        }

        private static void Execute(ref T data, IntPtr additionalPtr, IntPtr bufferRangePatchData, ref JobRanges ranges, int jobIndex) => data.Execute();
    }

    private struct WriteJob : IJob
    {
        public NativeArray<int> Data;
        public int Local;

        public void Execute()
        {
            Data[0]++;
            Data[1] = 42;
            Local = 5;
        }
    }

    private struct SetJob : IJob
    {
        public NativeArray<int> Data;
        public int Value;
        public void Execute() => Data[0] = Value;
    }

    private struct ScaleJob : IJob
    {
        public NativeArray<int> Data;
        public void Execute() => Data[0] = Data[0] * 10 + 2;
    }

    private struct FailingJob : IJob
    {
        public void Execute() => throw new InvalidOperationException("job failed");
    }

    private struct HitJob : IJobParallelFor
    {
        public NativeArray<int> Hits;
        public void Execute(int index) => Hits[index]++;
    }

    private struct ForHitJob : IJobFor
    {
        public NativeArray<int> Hits;
        public void Execute(int index) => Hits[index]++;
    }

    private struct OrderJob : IJobFor
    {
        public NativeArray<int> Order;
        public NativeArray<int> Cursor;

        public void Execute(int index)
        {
            if (Cursor[0] < Order.Length) Order[Cursor[0]] = index;
            Cursor[0]++;
        }
    }
}
