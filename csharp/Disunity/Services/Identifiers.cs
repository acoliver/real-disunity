using UnityEngine;

namespace UnityEngine.Identifiers;

/// <summary>The device identifiers the services packages read.</summary>
public static class Identifiers
{
    private const string InstallationFileName = "installation-id";
    private static readonly TimeSpan CreationLockTimeout = TimeSpan.FromSeconds(30);
    private static readonly TimeSpan LockRetryInterval = TimeSpan.FromMilliseconds(5);

    /// <summary>An identifier for this installation, stored in <see cref="Application.persistentDataPath"/>.
    /// It remains stable across processes that use the same persistent data path. Removing its file resets it.</summary>
    public static string installationId => LoadOrCreateInstallationId(Application.persistentDataPath);

    private static string LoadOrCreateInstallationId(string persistentDataPath)
    {
        if (string.IsNullOrEmpty(persistentDataPath))
        {
            throw new InvalidOperationException("An installation identifier requires a persistent data path.");
        }

        Directory.CreateDirectory(persistentDataPath);
        var path = Path.Combine(persistentDataPath, InstallationFileName);
        if (File.Exists(path))
        {
            return ReadInstallationId(path);
        }

        // Concurrent creators serialize on a lock file. A rename that replaces an existing
        // file succeeds on some platforms, so creation re-checks for the file under the lock
        // and every process then reads the one identifier that was published.
        using var creationLock = AcquireCreationLock(path + ".lock");
        if (!File.Exists(path))
        {
            PublishInstallationId(persistentDataPath, path);
        }

        return ReadInstallationId(path);
    }

    private static void PublishInstallationId(string persistentDataPath, string path)
    {
        var temporaryPath = Path.Combine(persistentDataPath, $"{InstallationFileName}.{Guid.NewGuid():N}.tmp");
        try
        {
            File.WriteAllText(temporaryPath, Guid.NewGuid().ToString());
            File.Move(temporaryPath, path);
        }
        finally
        {
            File.Delete(temporaryPath);
        }
    }

    private static FileStream AcquireCreationLock(string lockPath)
    {
        var deadline = DateTime.UtcNow + CreationLockTimeout;
        while (true)
        {
            try
            {
                return new FileStream(lockPath, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None);
            }
            catch (IOException) when (File.Exists(lockPath) && DateTime.UtcNow < deadline)
            {
                Thread.Sleep(LockRetryInterval);
            }
        }
    }

    private static string ReadInstallationId(string path)
    {
        var value = File.ReadAllText(path);
        if (!Guid.TryParse(value, out var identifier) || identifier == Guid.Empty)
        {
            throw new InvalidDataException($"The installation identifier file '{path}' does not contain a nonzero GUID.");
        }

        return identifier.ToString();
    }
}
