using UnityEngine;

namespace UnityEngine.Identifiers;

/// <summary>The device identifiers the services packages read.</summary>
public static class Identifiers
{
    private const string InstallationFileName = "installation-id";

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

        var temporaryPath = Path.Combine(persistentDataPath, $"{InstallationFileName}.{Guid.NewGuid():N}.tmp");
        try
        {
            var installationId = Guid.NewGuid().ToString();
            File.WriteAllText(temporaryPath, installationId);
            try
            {
                File.Move(temporaryPath, path);
                return installationId;
            }
            catch (IOException) when (File.Exists(path))
            {
                return ReadInstallationId(path);
            }
        }
        finally
        {
            File.Delete(temporaryPath);
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
