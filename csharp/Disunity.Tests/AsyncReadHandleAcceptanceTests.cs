using System.Reflection;
using Unity.IO.LowLevel.Unsafe;

internal static unsafe class AsyncReadHandleAcceptanceTests
{
    public static void Run()
    {
        DefaultsRemainInvalidAfterCleanup();
        CommandsPreserveWideFileRegionsAndBufferBounds();
        RequestSubmissionIsExposed();
    }

    internal static void DefaultsRemainInvalidAfterCleanup()
    {
        var read = default(ReadHandle);
        var alias = read;
        read.Cancel();
        read.Dispose();
        alias.Cancel();
        alias.Dispose();
        read.Dispose();
        Expect.That(!read.IsValid() && !alias.IsValid(), "cleanup cannot manufacture a read request");
        Expect.Equal(ReadStatus.Failed, read.Status, "an invalid read remains failed after cleanup");
        Expect.Equal(0L, read.GetBytesRead(), "an invalid read cannot report transferred bytes");
        Expect.Equal(0L, alias.GetBytesRead(), "a copy of an invalid handle has no transferred bytes");
        var file = default(FileHandle);
        _ = file.Close();
        _ = file.Close();
        Expect.That(!file.IsValid(), "closing an invalid file cannot open one");
        Expect.Equal(FileState.Absent, file.Status, "an invalid file remains absent after closing");
    }

    internal static void CommandsPreserveWideFileRegionsAndBufferBounds()
    {
        byte* guarded = stackalloc byte[12];
        for (var i = 0; i < 12; i++) guarded[i] = 0xA5;
        var commands = stackalloc ReadCommand[2];
        var offset = (long)int.MaxValue + 17;
        var size = (long)int.MaxValue + 31;
        commands[0] = new ReadCommand { Buffer = guarded + 2, Offset = offset, Size = 8 };
        commands[1] = new ReadCommand { Buffer = null, Offset = 0, Size = size };
        var array = new ReadCommandArray { ReadCommands = commands, CommandCount = 2 };
        var alias = array;
        Expect.Equal(offset, alias.ReadCommands[0].Offset, "file offsets must not truncate to 32 bits");
        Expect.Equal(size, alias.ReadCommands[1].Size, "descriptor sizes must not truncate to 32 bits");
        Expect.That(alias.ReadCommands[0].Buffer == guarded + 2, "a copied array preserves its buffer address");
        Expect.That(alias.ReadCommands[1].Buffer == null, "descriptor construction does not invent storage");
        commands[0].Offset++;
        Expect.Equal(offset + 1, alias.ReadCommands[0].Offset, "the array borrows the command storage");
        array.ReadCommands = null;
        array.CommandCount = 0;
        Expect.That(alias.ReadCommands == commands && alias.CommandCount == 2, "array metadata is copied by value");
        for (var i = 0; i < 12; i++)
            Expect.Equal((byte)0xA5, guarded[i], "constructing descriptors must not write the borrowed target");
    }

    internal static void RequestSubmissionIsExposed()
    {
        var handleType = typeof(ReadHandle);
        var submissions = handleType.Assembly.GetExportedTypes()
            .SelectMany(type => type.GetMethods(BindingFlags.Public | BindingFlags.Static | BindingFlags.Instance))
            .Where(method => method.ReturnType == handleType && !method.IsSpecialName)
            .ToArray();
        Expect.That(submissions.Length > 0,
            "async-read acceptance: no public operation can create a ReadHandle; invalid defaults do not execute a file-region read");
    }
}
