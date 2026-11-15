using Disunity.Hosting;
using Disunity.Native;

internal static class AbiRevisionTests
{
    internal static void Run()
    {
        Expect.That(!DisunityHost.IsLoaded, "ABI verification precedes world creation");
        Expect.That(NativeAbi.Version == 11, "distance draw ABI revision");
        NativeAbi.VerifyRevision(11);
        foreach (var actual in new uint[] { 10, 12 })
        {
            var error = Expect.Catch<DisunityAbiRevisionException>(
                () => NativeAbi.VerifyRevision(actual), "mismatched native revision");
            Expect.That(error.Message == $"ABI revision mismatch: expected 11, actual {actual}", "expected and actual ABI diagnostic");
            Expect.That(error.Expected == 11, "expected revision field");
            Expect.That(error.Actual == actual, "actual revision field");
            Expect.That(!DisunityHost.IsLoaded, "mismatched revision creates no world");
        }
        Console.WriteLine("PASS: managed ABI revision mismatch names expected and actual revisions before world creation");
    }
}
