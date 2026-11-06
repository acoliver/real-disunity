using Unity.IO.LowLevel.Unsafe;

/// <summary>ReadCommand and ReadCommandArray are plain data that point at the reads they hold.</summary>
internal static unsafe class AsyncReadTypeTests
{
    public static void Run()
    {
        var buffer = stackalloc byte[8];
        var commands = stackalloc ReadCommand[2];
        commands[0] = new ReadCommand { Buffer = buffer, Offset = 4, Size = 8 };
        commands[1] = new ReadCommand { Buffer = buffer + 4, Offset = 64, Size = 2 };
        var array = new ReadCommandArray { ReadCommands = commands, CommandCount = 2 };
        Expect.Equal(2u, array.CommandCount, "the count");
        Expect.That(array.ReadCommands[1].Offset == 64 && array.ReadCommands[1].Size == 2 && array.ReadCommands[1].Buffer == buffer + 4, "the second command");
        Expect.That(array.ReadCommands[0].Buffer == buffer && array.ReadCommands[0].Offset == 4 && array.ReadCommands[0].Size == 8, "the first command");
        Expect.That(default(ReadCommandArray).ReadCommands == null && default(ReadCommandArray).CommandCount == 0, "a default array holds no command");
        Console.WriteLine("PASS: ReadCommand and ReadCommandArray hold the reads they point at");
    }
}
