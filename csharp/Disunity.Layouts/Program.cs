using System.Text.Json;
using Disunity.Hosting;

// Disunity.Layouts <player data directory> <output JSON>: writes the script layouts of the
// player's game and package assemblies, for unity-import to decode script fields with.
if (args.Length != 2)
{
    Console.Error.WriteLine("usage: Disunity.Layouts <player data directory> <output JSON>");
    return 2;
}
var manifest = PlayerManifest.Read(args[0]);
var managed = Path.Combine(args[0], "Managed");
var images = Directory.GetFiles(managed, "*.dll")
    .Select(path => (Path.GetFileNameWithoutExtension(path), File.ReadAllBytes(path)))
    .ToList();
using var layouts = new ScriptLayouts(images);
var scripts = manifest.GameAssemblies.SelectMany(layouts.Scripts).ToList();
using (var stream = File.Create(args[1]))
using (var writer = new Utf8JsonWriter(stream))
    layouts.Write(writer, scripts);
// Fields of these types are left out of their layouts, so the summary names them.
Console.WriteLine($"{scripts.Count} scripts, {layouts.Types.Count} types, {layouts.Unresolved.Count} unresolved types");
foreach (var name in layouts.Unresolved) Console.WriteLine($"unresolved type: {name}");
return 0;
