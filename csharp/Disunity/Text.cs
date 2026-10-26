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

    private unsafe DisunityTextStyle Style()
    {
        DisunityTextStyle value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_text_style_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    private unsafe void Update(Func<DisunityTextStyle, DisunityTextStyle> change)
    {
        var value = change(Style());
        DisunityHost.ThrowIfError(NativeBindings.disunity_text_style_write(DisunityHost.WorldHandle, Handle, &value));
    }
    public Font font
    {
        get => DisunityHost.ResolveFont(Style().font);
        set
        {
            ArgumentNullException.ThrowIfNull(value);
            if (value.World != DisunityHost.WorldHandle) throw new ArgumentException("font belongs to another world", nameof(value));
            Update(s => { s.font = value.Handle; return s; });
        }
    }
    public Color color
    {
        get { var s = Style(); return new Color(s.r, s.g, s.b, s.a); }
        set => Update(s => { (s.r, s.g, s.b, s.a) = (value.r, value.g, value.b, value.a); return s; });
    }
    public TextAnchor alignment { get => (TextAnchor)Style().alignment; set => Update(s => { s.alignment = (uint)value; return s; }); }
    public float lineSpacing { get => Style().line_spacing; set => Update(s => { s.line_spacing = value; return s; }); }
    public HorizontalWrapMode horizontalOverflow { get => (HorizontalWrapMode)Style().horizontal_overflow; set => Update(s => { s.horizontal_overflow = (uint)value; return s; }); }
    public VerticalWrapMode verticalOverflow { get => (VerticalWrapMode)Style().vertical_overflow; set => Update(s => { s.vertical_overflow = (uint)value; return s; }); }
    /// <summary>Only <see cref="FontStyle.Normal"/> is supported; other styles are rejected.</summary>
    public FontStyle fontStyle { get => (FontStyle)Style().font_style; set => Update(s => { s.font_style = (uint)value; return s; }); }
    /// <summary>On by default as in Unity. Markup is rejected while it is on.</summary>
    public bool supportRichText { get => Style().rich_text == 1; set => Update(s => { s.rich_text = value ? 1u : 0u; return s; }); }
    /// <summary>Best fit is not supported; enabling it is rejected.</summary>
    public bool resizeTextForBestFit { get => Style().best_fit == 1; set => Update(s => { s.best_fit = value ? 1u : 0u; return s; }); }
}
