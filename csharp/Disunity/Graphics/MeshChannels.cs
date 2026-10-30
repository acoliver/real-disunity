namespace UnityEngine;

public partial class Mesh
{
    public Vector3[] vertices
    {
        get => ReadPositions();
        set => StorePositions(value);
    }

    public Vector3[] normals
    {
        get => Read("normals", _normals);
        set => StoreNormals(value);
    }

    public Vector4[] tangents
    {
        get => Read("tangents", _tangents);
        set => StoreTangents(value);
    }

    public Color[] colors
    {
        get => Read("colors", _colors);
        set => StoreColors("colors", value);
    }

    public Color32[] colors32
    {
        get => ReadColors32();
        set => StoreColors("colors32", Map(value, color => (Color)color));
    }

    public Vector2[] uv { get => ReadUv(0); set => SetUVs(0, value); }
    public Vector2[] uv2 { get => ReadUv(1); set => SetUVs(1, value); }
    public Vector2[] uv3 { get => ReadUv(2); set => SetUVs(2, value); }
    public Vector2[] uv4 { get => ReadUv(3); set => SetUVs(3, value); }
    public Vector2[] uv5 { get => ReadUv(4); set => SetUVs(4, value); }
    public Vector2[] uv6 { get => ReadUv(5); set => SetUVs(5, value); }
    public Vector2[] uv7 { get => ReadUv(6); set => SetUVs(6, value); }
    public Vector2[] uv8 { get => ReadUv(7); set => SetUVs(7, value); }

    public void SetVertices(Vector3[] inVertices) => StorePositions(inVertices);
    public void SetVertices(List<Vector3> inVertices) => StorePositions(inVertices);
    public void GetVertices(List<Vector3> vertices) => Fill(vertices, ReadPositions());

    public void SetNormals(Vector3[] inNormals) => StoreNormals(inNormals);
    public void SetNormals(List<Vector3> inNormals) => StoreNormals(inNormals);
    public void GetNormals(List<Vector3> normals) => Fill(normals, Read("normals", _normals));

    public void SetTangents(Vector4[] inTangents) => StoreTangents(inTangents);
    public void SetTangents(List<Vector4> inTangents) => StoreTangents(inTangents);
    public void GetTangents(List<Vector4> tangents) => Fill(tangents, Read("tangents", _tangents));

    public void SetColors(Color[] inColors) => StoreColors("colors", inColors);
    public void SetColors(List<Color> inColors) => StoreColors("colors", inColors);
    public void SetColors(Color32[] inColors) => StoreColors("colors", Map(inColors, color => (Color)color));
    public void SetColors(List<Color32> inColors) => StoreColors("colors", Map(inColors, color => (Color)color));
    public void GetColors(List<Color> colors) => Fill(colors, Read("colors", _colors));
    public void GetColors(List<Color32> colors) => Fill(colors, ReadColors32());

    public void SetUVs(int channel, Vector2[] uvs) => StoreUvs(channel, Map(uvs, value => (Vector4)value), 2);
    public void SetUVs(int channel, List<Vector2> uvs) => StoreUvs(channel, Map(uvs, value => (Vector4)value), 2);
    public void SetUVs(int channel, Vector3[] uvs) => StoreUvs(channel, Map(uvs, value => (Vector4)value), 3);
    public void SetUVs(int channel, List<Vector3> uvs) => StoreUvs(channel, Map(uvs, value => (Vector4)value), 3);
    public void SetUVs(int channel, Vector4[] uvs) => StoreUvs(channel, Map(uvs, value => value), 4);
    public void SetUVs(int channel, List<Vector4> uvs) => StoreUvs(channel, Map(uvs, value => value), 4);

    public void GetUVs(int channel, List<Vector2> uvs) => Fill(uvs, Map(ReadUvChannel(channel), value => (Vector2)value));
    public void GetUVs(int channel, List<Vector3> uvs) => Fill(uvs, Map(ReadUvChannel(channel), value => (Vector3)value));
    public void GetUVs(int channel, List<Vector4> uvs) => Fill(uvs, ReadUvChannel(channel));

    /// <summary>The engine's choice: a channel is validated against the current vertex count, so the positions are set first.</summary>
    private T[] Validated<T>(string channel, T[] values)
    {
        RequireReadable();
        if (values.Length != 0 && values.Length != vertexCount)
        {
            throw new ArgumentException($"{channel}: {values.Length} values do not match the mesh's {vertexCount} vertices; a channel is empty or has one value per vertex");
        }
        return values;
    }

    private void StorePositions(IReadOnlyList<Vector3>? values)
    {
        RequireReadable();
        var positions = Map(values, value => value);
        if (positions.Length != vertexCount)
        {
            CheckIndicesFit(positions.Length);
            DropMismatchedChannels(positions.Length);
        }
        _positions = positions;
    }

    private void StoreNormals(IReadOnlyList<Vector3>? values) => _normals = Validated("normals", Map(values, value => value));

    private void StoreTangents(IReadOnlyList<Vector4>? values) => _tangents = Validated("tangents", Map(values, value => value));

    private void StoreColors(string channel, IReadOnlyList<Color>? values) => _colors = Validated(channel, Map(values, value => value));

    private void StoreUvs(int channel, Vector4[] values, int dimension)
    {
        CheckUvChannel(channel);
        _uvs[channel] = Validated(UvName(channel), values);
        _uvDimensions[channel] = values.Length == 0 ? 0 : dimension;
    }

    private void DropMismatchedChannels(int newCount)
    {
        if (_normals.Length != newCount) _normals = [];
        if (_tangents.Length != newCount) _tangents = [];
        if (_colors.Length != newCount) _colors = [];
        for (var channel = 0; channel < UvChannelCount; channel++)
        {
            if (_uvs[channel].Length == newCount) continue;
            _uvs[channel] = [];
            _uvDimensions[channel] = 0;
        }
    }

    private Vector3[] ReadPositions() => Read("vertices", _positions);

    private T[] Read<T>(string channel, T[] values)
    {
        RequireReadable();
        return (T[])values.Clone();
    }

    private Color32[] ReadColors32()
    {
        RequireReadable();
        return Map(_colors, color => (Color32)color);
    }

    private Vector2[] ReadUv(int channel) => Map(ReadUvChannel(channel), value => (Vector2)value);

    private Vector4[] ReadUvChannel(int channel)
    {
        CheckUvChannel(channel);
        return Read(UvName(channel), _uvs[channel]);
    }

    private static void CheckUvChannel(int channel)
    {
        if (channel < 0 || channel >= UvChannelCount)
        {
            throw new ArgumentOutOfRangeException(nameof(channel), channel, "a uv channel is 0 to 7");
        }
    }

    /// <summary>The property name of a channel: uv for channel 0, then uv2 to uv8.</summary>
    private static string UvName(int channel) => channel == 0 ? "uv" : "uv" + (channel + 1);

    private static TOut[] Map<TIn, TOut>(IReadOnlyList<TIn>? source, Func<TIn, TOut> convert)
    {
        if (source == null || source.Count == 0) return [];
        var result = new TOut[source.Count];
        for (var index = 0; index < result.Length; index++) result[index] = convert(source[index]);
        return result;
    }

    private static void Fill<T>(List<T> target, T[] values)
    {
        ArgumentNullException.ThrowIfNull(target);
        target.Clear();
        target.AddRange(values);
    }
}
