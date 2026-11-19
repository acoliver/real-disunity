using System.Runtime.InteropServices;
using Unity.IO.LowLevel.Unsafe;

/// <summary>Executed by the native runtime's focused file-read test, with its real worker installed.</summary>
public static unsafe class AsyncReadOwnedTests
{
    private static ReadHandle _survivor;

    [UnmanagedCallersOnly]
    public static int RunNative(nint path)
    {
        try
        {
            Console.WriteLine($"Managed async-read runtime: {RuntimeInformation.FrameworkDescription}");
            Run(Marshal.PtrToStringUTF8(path)!);
            return 0;
        }
        catch (Exception error) { Console.Error.WriteLine(error); return 1; }
    }

    private static void Run(string path)
    {
        AsyncReadHandleAcceptanceTests.Run();
        PipelineTypeTests.DefaultHandlesAreInvalid();
        if (_survivor.Identity != 0)
        {
            Expect.That(!_survivor.IsValid(), "an old session cannot alias a new host's request identities");
            Expect.Throws<ObjectDisposedException>(() => _survivor.GetData(), "old session data is inaccessible after reattachment");
            _survivor.Dispose();
        }
        using var first = AsyncReadManager.Read(path, 3, 5);
        using var second = AsyncReadManager.Read(path, 8, 3);
        Expect.That(first.IsValid() && second.IsValid() && first.Identity != second.Identity, "real requests have distinct live identities");
        first.Complete();
        second.Complete();
        Expect.Equal(ReadStatus.Complete, first.Status, "file region completed");
        Expect.Equal(5L, first.GetBytesRead(), "exact native transfer count");
        Expect.That(first.Error is null && first.GetData().SequenceEqual("34567"u8.ToArray()), "exact native range bytes and no error");
        Expect.That(second.GetData().SequenceEqual("89a"u8.ToArray()), "a second request owns independent data");
        var copy = first.GetData();
        copy[0] = 0;
        Expect.Equal((byte)'3', first.GetData()[0], "returned buffers are independent owned copies");
        first.Cancel();
        Expect.Equal(ReadStatus.Complete, first.Status, "late cancel cannot rewrite success");
        Expect.Throws<NotSupportedException>(() => { _ = first.JobHandle; }, "no fabricated completed job for native I/O");
        ErrorsAndCancellation(path);
        var alias = second;
        second.Dispose();
        alias.Dispose();
        alias.Cancel();
        Expect.That(!alias.IsValid() && !second.IsValid(), "copies share disposal");
        Expect.Equal(ReadStatus.Failed, alias.Status, "disposed handles cannot claim success");
        Expect.Equal(0L, alias.GetBytesRead(), "disposed handle has no readable result");
        Expect.Throws<ObjectDisposedException>(() => alias.GetData(), "disposed native storage cannot be copied");
        BorrowedPointersAreRejected(path);
        Expect.Throws<ArgumentOutOfRangeException>(() => AsyncReadManager.Read(path, -1, 1), "negative offsets rejected");
        Expect.Throws<ArgumentOutOfRangeException>(() => AsyncReadManager.Read(path, 0, -1), "negative sizes rejected");
        Expect.Throws<ArgumentOutOfRangeException>(() => AsyncReadManager.Read(path, long.MaxValue, 1), "signed offset overflow rejected");
        Expect.Throws<ArgumentException>(() => AsyncReadManager.Read(new string('x', 4097), 0, 1), "path byte budget rejected before submission");
        Expect.Throws<ArgumentException>(() => AsyncReadManager.Read(path, 0, 16 * 1024 * 1024 + 1), "native per-read admission limit");
        _survivor = AsyncReadManager.Read(path, 0, 1);
        _survivor.Complete();
        Console.WriteLine("PASS: owned managed file reads execute in the native runtime worker");
    }

    private static void ErrorsAndCancellation(string path)
    {
        using var missing = AsyncReadManager.Read(path + ".missing", 0, 4);
        missing.Complete();
        missing.Cancel();
        Expect.Equal(ReadStatus.Failed, missing.Status, "missing file reports failure");
        Expect.That(missing.Error is { Length: > 0 }, "missing-file error is exposed");
        Expect.Equal(0L, missing.GetBytesRead(), "missing file transfers no bytes");
        Expect.That(missing.GetData().Length == 0, "missing file has no fabricated payload");
        using var shortRead = AsyncReadManager.Read(path, 14, 4);
        shortRead.Complete();
        Expect.Equal(ReadStatus.Failed, shortRead.Status, "short region fails");
        Expect.Equal(2L, shortRead.GetBytesRead(), "partial EOF transfer count");
        Expect.That(shortRead.Error is { Length: > 0 } && shortRead.GetData().SequenceEqual("ef"u8.ToArray()), "partial EOF bytes and diagnostic");
        using var empty = AsyncReadManager.Read(path, 16, 0);
        empty.Complete();
        Expect.That(empty.Status == ReadStatus.Complete && empty.GetBytesRead() == 0 && empty.GetData().Length == 0, "zero-byte region completes truthfully");
        using var canceled = AsyncReadManager.Read(path, 0, 16);
        canceled.Cancel();
        canceled.Complete();
        Expect.That(canceled.Status is ReadStatus.Canceled or ReadStatus.Complete, "best-effort cancellation acknowledges the actual terminal outcome");
        var count = canceled.GetBytesRead();
        Expect.That(canceled.GetData().SequenceEqual("0123456789abcdef"u8[..(int)count].ToArray()), "cancellation preserves actual transferred prefix");
        Expect.That((canceled.Status == ReadStatus.Complete) == (count == 16), "full transfer wins over late cancellation");
    }

    private static void BorrowedPointersAreRejected(string path)
    {
        var guarded = stackalloc byte[8];
        for (var i = 0; i < 8; i++) guarded[i] = 0xa5;
        var command = new ReadCommand { Buffer = guarded + 2, Offset = 0, Size = 4 };
        try { AsyncReadManager.Read(path, &command, 1); throw new Exception("raw pointer submission was accepted"); }
        catch (NotSupportedException) { }
        for (var i = 0; i < 8; i++) Expect.Equal((byte)0xa5, guarded[i], "rejected raw targets remain untouched");
    }

    [UnmanagedCallersOnly]
    public static int AfterDetach()
    {
        try
        {
            Expect.That(!_survivor.IsValid(), "host teardown invalidates old session handles");
            Expect.Equal(ReadStatus.Failed, _survivor.Status, "ended session cannot report completion");
            _survivor.Cancel();
            Expect.Throws<InvalidOperationException>(() => AsyncReadManager.Read("unused", 0, 1), "no submission outside an installed host");
            return 0;
        }
        catch (Exception error) { Console.Error.WriteLine(error); return 1; }
    }
}
