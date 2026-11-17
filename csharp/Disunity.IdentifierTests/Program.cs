using System.Diagnostics;
using Disunity.Hosting;
using UnityEngine.Identifiers;

if (args is ["read", var readPath])
{
    Configure(readPath);
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

var concurrent = Enumerable.Range(0, 8)
    .Select(_ => StartChild("read", Path.Combine(root.Path, "concurrent")))
    .ToArray();
var concurrentIds = await Task.WhenAll(concurrent.Select(ReadChildOutput));
Require(concurrentIds.All(value => value == concurrentIds[0]), "concurrent processes create one shared id");

var blockedScope = Path.Combine(root.Path, "not-a-directory");
File.WriteAllText(blockedScope, "fixture");
var failure = RunChild("failure", blockedScope);
var corruptScope = Path.Combine(root.Path, "corrupt");
Directory.CreateDirectory(corruptScope);
var corruptFile = Path.Combine(corruptScope, "installation-id");
File.WriteAllText(corruptFile, "invalid");
var corruptFailure = RunChild("failure", corruptScope);
Require(corruptFailure == typeof(InvalidDataException).FullName && File.ReadAllText(corruptFile) == "invalid",
    "a corrupt stored id fails without being replaced");

Require(failure == typeof(IOException).FullName || failure == typeof(UnauthorizedAccessException).FullName,
    $"a storage failure is reported instead of replaced with a transient id: {failure}");

Console.WriteLine("PASS: installation identity persists across processes, scopes, reset, concurrency and storage failure");

static void Configure(string path)
{
    DisunityHost.ConfigurePlayer(PlayerEnvironment.Default with { PersistentDataPath = path });
}

static string RunChild(string command, string path)
{
    var process = StartChild(command, path);
    var output = ReadChildOutput(process).GetAwaiter().GetResult();
    return output;
}

static Process StartChild(string command, string path)
{
    var process = Process.Start(new ProcessStartInfo("dotnet")
    {
        ArgumentList = { typeof(Program).Assembly.Location, command, path },
        RedirectStandardOutput = true,
        RedirectStandardError = true,
        UseShellExecute = false
    }) ?? throw new InvalidOperationException("Could not start the identifier fixture process.");
    return process;
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
