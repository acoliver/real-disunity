using System.Globalization;
using System.Xml.Linq;

namespace UnityEngine;

/// <summary>
/// Named values kept between runs. Each key holds an <c>int</c>, a <c>float</c> or a string;
/// reading a key as another type gives the default. Values are kept in memory and written to
/// <c>PlayerPrefs.xml</c> in the persistent data path by <see cref="Save"/>, which Unity also
/// does when the game quits. The file holds one <c>&lt;pref name type&gt;</c> element per key,
/// in the shape of the preferences file Unity writes on Linux. Without a persistent data path
/// nothing is written. A file that cannot be read is reported and the game starts with none.
/// </summary>
public class PlayerPrefs
{
    private const string FileName = "PlayerPrefs.xml";
    private static readonly object Gate = new();
    private static Dictionary<string, object> _values = new(StringComparer.Ordinal);
    private static string _directory = "";

    /// <summary>Loads the preferences saved in <paramref name="directory"/>, or starts empty.</summary>
    internal static void Open(string directory)
    {
        var path = Path.Combine(directory, FileName);
        var values = directory.Length > 0 && File.Exists(path) ? Load(path) : new(StringComparer.Ordinal);
        lock (Gate)
        {
            _values = values;
            _directory = directory;
        }
    }

    private static Dictionary<string, object> Load(string path)
    {
        var values = new Dictionary<string, object>(StringComparer.Ordinal);
        try
        {
            foreach (var pref in XDocument.Load(path).Root?.Elements("pref") ?? [])
                values[(string?)pref.Attribute("name") ?? throw new InvalidDataException("a pref has no name")] = Read(pref);
        }
        catch (Exception error) when (error is System.Xml.XmlException or InvalidDataException or FormatException or OverflowException)
        {
            Debug.LogWarning($"PlayerPrefs: {path} cannot be read and is ignored: {error.Message}");
            values.Clear();
        }
        return values;
    }

    private static object Read(XElement pref) => (string?)pref.Attribute("type") switch
    {
        "int" => int.Parse(pref.Value, CultureInfo.InvariantCulture),
        "float" => float.Parse(pref.Value, NumberStyles.Float, CultureInfo.InvariantCulture),
        "string" => pref.Value,
        var other => throw new InvalidDataException($"unknown pref type {other}"),
    };

    public static void SetInt(string key, int value) => Set(key, value);
    public static void SetFloat(string key, float value) => Set(key, value);
    public static void SetString(string key, string value) => Set(key, value ?? throw new ArgumentNullException(nameof(value)));
    public static int GetInt(string key) => GetInt(key, 0);
    public static int GetInt(string key, int defaultValue) => Get(key, defaultValue);
    public static float GetFloat(string key) => GetFloat(key, 0);
    public static float GetFloat(string key, float defaultValue) => Get(key, defaultValue);
    public static string GetString(string key) => GetString(key, "");
    public static string GetString(string key, string defaultValue) => Get(key, defaultValue);

    public static bool HasKey(string key)
    {
        lock (Gate) return _values.ContainsKey(key);
    }

    public static void DeleteKey(string key)
    {
        lock (Gate) _values.Remove(key);
    }

    public static void DeleteAll()
    {
        lock (Gate) _values.Clear();
    }

    /// <summary>Writes the preferences to a temporary file and moves it into place, so a
    /// failure leaves the previous file whole.</summary>
    public static void Save()
    {
        string directory;
        XDocument document;
        lock (Gate)
        {
            directory = _directory;
            if (directory.Length == 0) return;
            document = new XDocument(new XElement("unity_prefs",
                new XAttribute("version_major", 1), new XAttribute("version_minor", 1),
                _values.Select(entry => Saved(entry.Key, entry.Value))));
        }
        Directory.CreateDirectory(directory);
        var path = Path.Combine(directory, FileName);
        var temporary = path + ".tmp";
        document.Save(temporary);
        File.Move(temporary, path, overwrite: true);
    }

    private static XElement Saved(string key, object value) => value switch
    {
        int number => new XElement("pref", new XAttribute("name", key), new XAttribute("type", "int"), number.ToString(CultureInfo.InvariantCulture)),
        float number => new XElement("pref", new XAttribute("name", key), new XAttribute("type", "float"), number.ToString("R", CultureInfo.InvariantCulture)),
        _ => new XElement("pref", new XAttribute("name", key), new XAttribute("type", "string"), (string)value),
    };

    private static void Set(string key, object value)
    {
        ArgumentNullException.ThrowIfNull(key);
        lock (Gate) _values[key] = value;
    }

    private static T Get<T>(string key, T defaultValue)
    {
        lock (Gate) return _values.TryGetValue(key, out var value) && value is T typed ? typed : defaultValue;
    }
}
