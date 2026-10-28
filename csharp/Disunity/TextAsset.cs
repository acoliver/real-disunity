using System.Text;

namespace UnityEngine;

/// <summary>
/// Text or binary data from a file in the project. <see cref="text"/> decodes the bytes as
/// Unity does: a UTF-32, UTF-8 or UTF-16 byte order mark selects that encoding and is not
/// part of the text, and bytes without one are UTF-8.
/// </summary>
public class TextAsset : Object
{
    private readonly byte[] _bytes;
    private string? _text;

    public TextAsset() : this("") { }

    public TextAsset(string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        _bytes = Encoding.UTF8.GetBytes(text);
        _text = text;
    }

    internal TextAsset(string name, byte[] bytes)
    {
        this.name = name;
        _bytes = bytes;
    }

    public byte[] bytes => (byte[])_bytes.Clone();
    public long dataSize => _bytes.LongLength;
    public string text => _text ??= Decode(_bytes);
    internal override Object CloneAsset() => new TextAsset(name + "(Clone)", _bytes) { _text = _text };
    public override string ToString() => text;

    private static readonly Encoding[] Marked =
    [
        new UTF32Encoding(bigEndian: true, byteOrderMark: true),
        new UTF32Encoding(bigEndian: false, byteOrderMark: true),
        new UTF8Encoding(encoderShouldEmitUTF8Identifier: true),
        new UnicodeEncoding(bigEndian: true, byteOrderMark: true),
        new UnicodeEncoding(bigEndian: false, byteOrderMark: true),
    ];

    private static string Decode(byte[] bytes)
    {
        foreach (var encoding in Marked)
        {
            var mark = encoding.Preamble;
            if (bytes.AsSpan().StartsWith(mark)) return encoding.GetString(bytes, mark.Length, bytes.Length - mark.Length);
        }
        return Encoding.UTF8.GetString(bytes);
    }
}
