using Disunity.Hosting;
using Disunity.Native;

internal static class AbiRevisionTests
{
    internal static void Run()
    {
        Expect.That(!DisunityHost.IsLoaded, "ABI verification precedes world creation");
        Expect.That(NativeAbi.Version == 19, "current ABI revision");
        NativeAbi.VerifyRevision(19);
        foreach (var actual in new uint[] { 18, 20 })
        {
            var error = Expect.Catch<DisunityAbiRevisionException>(
                () => NativeAbi.VerifyRevision(actual), "mismatched native revision");
            Expect.That(error.Message == $"ABI revision mismatch: expected 19, actual {actual}", "expected and actual ABI diagnostic");
            Expect.That(error.Expected == 19, "expected revision field");
            Expect.That(error.Actual == actual, "actual revision field");
            Expect.That(!DisunityHost.IsLoaded, "mismatched revision creates no world");
        }
        Console.WriteLine("PASS: managed ABI revision mismatch names expected and actual revisions before world creation");
    }
}
