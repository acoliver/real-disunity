using Disunity.Hosting;
using Disunity.Native;

internal static class AbiRevisionTests
{
    internal static void Run()
    {
        Expect.That(!DisunityHost.IsLoaded, "ABI verification precedes world creation");
        // The vectors the native library's tests use, so both sides hash the surface the same way.
        Expect.Equal(0x811c9dc5u, NativeAbi.Revision(""u8), "FNV-1a of empty text");
        Expect.Equal(0xe40c292cu, NativeAbi.Revision("a"u8), "FNV-1a of a");
        Expect.Equal(0xbf9cf968u, NativeAbi.Revision("foobar"u8), "FNV-1a of foobar");
        var version = NativeAbi.Version;
        NativeAbi.VerifyRevision(version);
        foreach (var actual in new uint[] { unchecked(version - 1), unchecked(version + 1) })
        {
            var error = Expect.Catch<DisunityAbiRevisionException>(
                () => NativeAbi.VerifyRevision(actual), "mismatched native revision");
            Expect.That(error.Message == $"ABI revision mismatch: expected {version}, actual {actual}", "expected and actual ABI diagnostic");
            Expect.That(error.Expected == version, "expected revision field");
            Expect.That(error.Actual == actual, "actual revision field");
            Expect.That(!DisunityHost.IsLoaded, "mismatched revision creates no world");
        }
        Console.WriteLine("PASS: managed ABI revision mismatch names expected and actual revisions before world creation");
    }

    /// <summary>The managed revision is the one the native library reports; the native image must be configured.</summary>
    internal static unsafe void RunAgainstNative()
    {
        DisunityHost.EnsureLoaded();
        uint native;
        Expect.Equal(0, NativeBindings.disunity_abi_version(&native).code, "disunity_abi_version result");
        Expect.Equal(native, NativeAbi.Version, "native and managed ABI revisions");
        Console.WriteLine("PASS: the managed ABI revision equals the native library's");
    }
}
