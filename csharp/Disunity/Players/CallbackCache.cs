using System.Security.Cryptography;

namespace Disunity.Hosting;

/// <summary>
/// Holds the rewritten images of a player's game assemblies (<see cref="PlayerCallbackRewriter"/>) on disk, so the
/// load context reads a real file with the assembly's own file name, which games that copy assemblies by
/// <c>Assembly.Location</c> need. An image is stored as <c>&lt;directory&gt;/&lt;sha256 of the original, hex&gt;/&lt;file name&gt;</c>,
/// next to the original's symbol file when it has one, and a stored image is reused for an identical original.
/// </summary>
internal sealed class CallbackCache(string directory, Action<string>? log)
{
    /// <summary>
    /// The file to load for the game assembly at <paramref name="path"/>: the cached rewritten image, written now if
    /// the cache lacks it, or <paramref name="path"/> itself when the assembly has nothing to rewrite.
    /// </summary>
    public string Resolve(string name, string path)
    {
        var original = File.ReadAllBytes(path);
        var folder = Path.Combine(directory, Convert.ToHexString(SHA256.HashData(original)).ToLowerInvariant());
        var cached = Path.Combine(folder, Path.GetFileName(path));
        if (File.Exists(cached))
        {
            log?.Invoke($"native callbacks kept: {name} (cached)");
            return cached;
        }
        var rewrite = PlayerCallbackRewriter.Rewrite(original, Path.GetDirectoryName(path));
        if (rewrite.Failure is not null) log?.Invoke($"native callbacks not kept: {name} ({rewrite.Sites} sites): {rewrite.Failure}");
        if (rewrite.Image is null) return path;
        Directory.CreateDirectory(folder);
        CopySymbols(path, folder);
        Publish(cached, rewrite.Image);
        log?.Invoke($"native callbacks kept: {name} ({rewrite.Sites} sites)");
        return cached;
    }

    private static void CopySymbols(string path, string folder)
    {
        var symbols = Path.ChangeExtension(path, ".pdb");
        if (File.Exists(symbols)) File.Copy(symbols, Path.Combine(folder, Path.GetFileName(symbols)), overwrite: true);
    }

    /// <summary>
    /// Writes the image beside its final name and moves it into place, so a file that exists is complete, even when
    /// two hosts fill the cache at once.
    /// </summary>
    private static void Publish(string cached, byte[] image)
    {
        var temporary = $"{cached}.{Guid.NewGuid():N}.tmp";
        File.WriteAllBytes(temporary, image);
        try
        {
            File.Move(temporary, cached);
        }
        catch (IOException) when (File.Exists(cached))
        {
            File.Delete(temporary);
        }
    }
}
