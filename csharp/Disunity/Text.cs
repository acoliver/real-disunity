using System.Text;
using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine.UI;

public sealed class Text : NativeComponent
{
    internal Text() { }
    private unsafe (string Text, float Size) Read()
    {
        uint length;
        float size;
        var probe = NativeBindings.disunity_text_read(DisunityHost.WorldHandle, Handle, null, 0, &length, &size);
        if (probe.code != 2) DisunityHost.ThrowIfError(probe);
        var bytes = new byte[length];
        uint written;
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_text_read(DisunityHost.WorldHandle, Handle, pointer, length, &written, &size));
        if (written != length) throw new InvalidOperationException("native text length changed during copy");
        return (Encoding.UTF8.GetString(bytes), size);
    }
    private unsafe void Write(string value, float size)
    {
        ArgumentNullException.ThrowIfNull(value);
        var bytes = Encoding.UTF8.GetBytes(value);
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_text_write(DisunityHost.WorldHandle, Handle, size, pointer, checked((uint)bytes.Length)));
    }
    public string text { get => Read().Text; set => Write(value, Read().Size); }
    public int fontSize { get => checked((int)Read().Size); set => Write(Read().Text, value); }
}
