using System.Diagnostics;
using Disunity.Hosting;
using UnityEngine.Identifiers;

if (args is ["read", var readPath])
{
    Configure(readPath);
    Console.WriteLine(Identifiers.installationId);
    return;
}

if (args is ["barrier", var barrierPath])
{
    Configure(barrierPath);
    Console.WriteLine("READY");
    Console.Out.Flush();
    if (Console.ReadLine() != "release")
    {
        throw new InvalidOperationException("The concurrent-start fixture was not released.");
    }

    Console.WriteLine(Identifiers.installationId);
    return;
}

if (args is ["empty", _])
{
    Configure("");
    Console.WriteLine(Identifiers.installationId);
    return;
}

if (args is ["failure", var failurePath])
{
    Configure(failurePath);
    try
    {
        _ = Identifiers.installationId;
        return;
    }
    catch (Exception exception)
    {
        Console.WriteLine(exception.GetType().FullName);
        return;
    }
}

if (args.Length != 0)
{
    throw new ArgumentException("Expected no arguments or a focused child command.");
}

await RunCoverage();

static async Task RunCoverage()
{
    using var root = new TemporaryDirectory();
    var firstScope = Path.Combine(root.Path, "first");
    var secondScope = Path.Combine(root.Path, "second");
    var first = RunChild("read", firstScope);
    var second = RunChild("read", firstScope);
    Require(Guid.TryParse(first, out var firstId) && firstId != Guid.Empty, "the installation id is a nonzero GUID");
    Require(first == second, "a second process in the same scope reads the saved id");
    var otherScope = RunChild("read", secondScope);
    Require(Guid.TryParse(otherScope, out var otherId) && otherId != Guid.Empty && otherId != firstId,
        "a different persistent-data scope has its own id");
    File.Delete(Path.Combine(firstScope, "installation-id"));
    var reset = RunChild("read", firstScope);
    Require(Guid.TryParse(reset, out var resetId) && resetId != Guid.Empty && reset != first,
        "removing the stored file resets the installation id");
    VerifyScopeSwitching(firstScope, secondScope, reset, otherScope);
    await VerifyConcurrentCreation(Path.Combine(root.Path, "concurrent"));
    VerifyEmptyPath();
    VerifyStorageFailures(root.Path);
    Console.WriteLine("PASS: installation identity persists across processes, scopes, reset, concurrency and storage failure");
}

static void VerifyScopeSwitching(string firstScope, string secondScope, string firstId, string secondId)
{
    Configure(firstScope);
    var firstRead = Identifiers.installationId;
    Configure(secondScope);
    var secondRead = Identifiers.installationId;
    Configure(firstScope);
    Require(firstRead == firstId && secondRead == secondId && Identifiers.installationId == firstRead,
        "switching persistent-data scopes in one process reads each scope's stored id");
}

static async Task VerifyConcurrentCreation(string path)
{
    var processes = Enumerable.Range(0, 8).Select(_ => StartChild("barrier", path)).ToArray();
    await Task.WhenAll(processes.Select(WaitUntilReady));
    foreach (var process in processes)
    {
        await process.StandardInput.WriteLineAsync("release");
        process.StandardInput.Close();
    }

    var identifiers = await Task.WhenAll(processes.Select(ReadChildOutput));
    Require(Guid.TryParse(identifiers[0], out var identifier) && identifier != Guid.Empty
        && identifiers.All(value => value == identifiers[0]), "concurrent processes create one shared nonzero id");
    Require(RunChild("read", path) == identifiers[0],
        "a fresh process reads the identifier created by concurrent processes");
}

static void VerifyEmptyPath()
{
    var process = StartChild("empty", "unused");
    var output = process.StandardOutput.ReadToEnd();
    var error = process.StandardError.ReadToEnd();
    process.WaitForExit();
    Require(process.ExitCode != 0 && (output + error).Contains(nameof(InvalidOperationException)),
        $"an empty persistent-data path fails explicitly in a subprocess: exit {process.ExitCode}, output {output}, error {error}");
}

static void VerifyStorageFailures(string root)
{
    var blockedScope = Path.Combine(root, "not-a-directory");
    File.WriteAllText(blockedScope, "fixture");
    var failure = RunChild("failure", blockedScope);
    var corruptFile = Path.Combine(root, "corrupt", "installation-id");
    Directory.CreateDirectory(Path.GetDirectoryName(corruptFile)!);
    File.WriteAllText(corruptFile, "invalid");
    Require(RunChild("failure", Path.GetDirectoryName(corruptFile)!) == typeof(InvalidDataException).FullName
        && File.ReadAllText(corruptFile) == "invalid", "a corrupt stored id fails without being replaced");
    var zeroFile = Path.Combine(root, "zero", "installation-id");
    Directory.CreateDirectory(Path.GetDirectoryName(zeroFile)!);
    File.WriteAllText(zeroFile, Guid.Empty.ToString());
    Require(RunChild("failure", Path.GetDirectoryName(zeroFile)!) == typeof(InvalidDataException).FullName
        && File.ReadAllText(zeroFile) == Guid.Empty.ToString(),
        "a stored zero GUID fails the nonzero contract without being replaced");
    Require(failure == typeof(IOException).FullName || failure == typeof(UnauthorizedAccessException).FullName,
        $"a storage failure is reported instead of replaced with a transient id: {failure}");
}

static void Configure(string path)
{
    DisunityHost.ConfigurePlayer(PlayerEnvironment.Default with { PersistentDataPath = path });
}

static string RunChild(string command, string path)
{
    return ReadChildOutput(StartChild(command, path)).GetAwaiter().GetResult();
}

static Process StartChild(string command, string path)
{
    var process = Process.Start(new ProcessStartInfo("dotnet")
    {
        ArgumentList = { typeof(Program).Assembly.Location, command, path },
        RedirectStandardOutput = true,
        RedirectStandardError = true,
        RedirectStandardInput = true,
        UseShellExecute = false
    });
    return process ?? throw new InvalidOperationException("Could not start the identifier fixture process.");
}

static async Task WaitUntilReady(Process process)
{
    var ready = await process.StandardOutput.ReadLineAsync();
    Require(ready == "READY", $"concurrent child reached the ready barrier: {ready}");
}

static async Task<string> ReadChildOutput(Process process)
{
    using (process)
    {
        var stdout = process.StandardOutput.ReadToEndAsync();
        var stderr = process.StandardError.ReadToEndAsync();
        await process.WaitForExitAsync();
        var output = (await stdout).Trim();
        var error = await stderr;
        Require(process.ExitCode == 0, $"fixture process failed ({process.ExitCode}): {error}");
        return output;
    }
}

static void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}

sealed class TemporaryDirectory : IDisposable
{
    public string Path { get; } = System.IO.Path.Combine(System.IO.Path.GetTempPath(), $"disunity-identifiers-{Guid.NewGuid():N}");

    public TemporaryDirectory() => Directory.CreateDirectory(Path);

    public void Dispose() => Directory.Delete(Path, recursive: true);
}
