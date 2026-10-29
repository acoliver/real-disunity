using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;

namespace UnityEngine;

/// <summary>Reads the machine the engine runs on. Each value is read once and kept.</summary>
internal static class HostMachine
{
    private const string Unavailable = SystemInfo.unsupportedIdentifier;

    private static readonly Lazy<Dictionary<string, string>> CpuInfo = new(ReadCpuInfo);
    private static readonly Lazy<string> UniqueId = new(ComputeUniqueId);
    private static readonly Lazy<string> OsText = new(DescribeOperatingSystem);

    internal static string OperatingSystem => OsText.Value;

    internal static OperatingSystemFamily Family =>
        System.OperatingSystem.IsWindows() ? OperatingSystemFamily.Windows
        : System.OperatingSystem.IsMacOS() ? OperatingSystemFamily.MacOSX
        : System.OperatingSystem.IsLinux() ? OperatingSystemFamily.Linux
        : OperatingSystemFamily.Other;

    /// <summary>The CPU model name from /proc/cpuinfo on Linux; elsewhere the process architecture, such as X64 or Arm64.</summary>
    internal static string ProcessorType =>
        CpuInfo.Value.TryGetValue("model name", out var name) ? name : RuntimeInformation.ProcessArchitecture.ToString();

    internal static string ProcessorManufacturer =>
        CpuInfo.Value.TryGetValue("vendor_id", out var vendor) ? vendor : Unavailable;

    /// <summary>The first core's current clock in MHz from /proc/cpuinfo; 0 where the file does not exist.</summary>
    internal static int ProcessorFrequency =>
        CpuInfo.Value.TryGetValue("cpu MHz", out var mhz)
        && double.TryParse(mhz, System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var value)
            ? (int)Math.Round(value) : 0;

    /// <summary>Total memory the runtime may use, in MB; a container or cgroup limit lowers it, as it limits the process.</summary>
    internal static int SystemMemoryMB =>
        (int)Math.Min(int.MaxValue, GC.GetGCMemoryInfo().TotalAvailableMemoryBytes / (1024 * 1024));

    internal static string DeviceName => Environment.MachineName;

    /// <summary>The DMI product name on Linux; "n/a" where the machine does not publish one.</summary>
    internal static string DeviceModel => ReadFirst("/sys/class/dmi/id/product_name") ?? Unavailable;

    /// <summary>A 40-character lowercase SHA-1 of /etc/machine-id (or the D-Bus copy) on Linux, or of the machine name
    /// elsewhere. The same machine gives the same value on every run.</summary>
    internal static string DeviceUniqueIdentifier => UniqueId.Value;

    private static string ComputeUniqueId()
    {
        var source = ReadFirst("/etc/machine-id") ?? ReadFirst("/var/lib/dbus/machine-id") ?? Environment.MachineName;
        return Convert.ToHexString(SHA1.HashData(Encoding.UTF8.GetBytes(source))).ToLowerInvariant();
    }

    /// <summary>
    /// Shaped like Unity's text: "Windows 10  (10.0.19045) 64bit", "Mac OS X 13.4.1" and "Linux 6.8.0-45-generic",
    /// where Linux gives the kernel release from /proc/sys/kernel/osrelease. The Windows name is "Windows 10"
    /// for any 10.x version and "Windows major.minor" before it; the bit count is the process's.
    /// </summary>
    private static string DescribeOperatingSystem()
    {
        var version = Environment.OSVersion.Version;
        if (System.OperatingSystem.IsWindows())
        {
            var name = version.Major >= 10 ? "Windows 10" : $"Windows {version.Major}.{version.Minor}";
            return $"{name}  ({version.Major}.{version.Minor}.{version.Build}) {(Environment.Is64BitOperatingSystem ? 64 : 32)}bit";
        }
        if (System.OperatingSystem.IsMacOS()) return $"Mac OS X {version.Major}.{version.Minor}.{Math.Max(0, version.Build)}";
        var kernel = ReadFirst("/proc/sys/kernel/osrelease");
        return kernel is null ? RuntimeInformation.OSDescription : $"Linux {kernel}";
    }

    private static string? ReadFirst(string path)
    {
        try
        {
            var text = File.ReadAllText(path).Trim();
            return text.Length == 0 ? null : text;
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
            return null;
        }
    }

    private static Dictionary<string, string> ReadCpuInfo()
    {
        var values = new Dictionary<string, string>();
        try
        {
            foreach (var line in File.ReadLines("/proc/cpuinfo"))
            {
                var colon = line.IndexOf(':', StringComparison.Ordinal);
                if (colon > 0) values.TryAdd(line[..colon].Trim(), line[(colon + 1)..].Trim());
            }
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException)
        {
        }
        return values;
    }
}
