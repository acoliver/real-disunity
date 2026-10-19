using System.Reflection;
using System.Runtime.InteropServices;

namespace Disunity;

/// <summary>Resolves all engine imports to the native image explicitly selected by the host.</summary>
internal static class NativeLibrary
{
    private static readonly object Gate = new();
    private static string? _path;
    private static bool _resolverSet;

    internal static void Configure(string path)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        var fullPath = Path.GetFullPath(path);
        if (!File.Exists(fullPath))
        {
            throw new FileNotFoundException("native engine library not found", fullPath);
        }
        lock (Gate)
        {
            if (_path != null && !string.Equals(_path, fullPath, StringComparison.Ordinal))
            {
                throw new InvalidOperationException("the native engine image cannot change in a live process");
            }
            _path = fullPath;
        }
    }

    internal static void EnsureLoaded()
    {
        lock (Gate)
        {
            if (_path == null)
            {
                throw new InvalidOperationException("configure the native engine library before using managed engine APIs");
            }
            if (!_resolverSet)
            {
                System.Runtime.InteropServices.NativeLibrary.SetDllImportResolver(
                    typeof(NativeLibrary).Assembly, Resolve);
                _resolverSet = true;
            }
        }
    }

    private static nint Resolve(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName != "worldrunner_disunity_ffi")
        {
            return 0;
        }
        lock (Gate)
        {
            return System.Runtime.InteropServices.NativeLibrary.Load(_path
                ?? throw new InvalidOperationException("native engine library is not configured"));
        }
    }
}
