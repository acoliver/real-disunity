internal static class TestArtifacts
{
    public static string Native { get; private set; } = "";
    public static string ManagedRoot { get; private set; } = "";
    public static string Repository { get; private set; } = "";

    public static void Configure(ref string[] args)
    {
        if (args.Length < 4 || args[0] != "--artifacts")
            throw new ArgumentException("usage: Compatibility.Tests --artifacts NATIVE MANAGED_ROOT REPOSITORY [suite]");
        Native = Path.GetFullPath(args[1]);
        ManagedRoot = Path.GetFullPath(args[2]);
        Repository = Path.GetFullPath(args[3]);
        if (!File.Exists(Native) || !Directory.Exists(ManagedRoot) || !Directory.Exists(Repository))
            throw new ArgumentException("artifact setup did not produce the selected paths");
        args = args[4..];
    }
}
