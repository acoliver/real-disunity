using System.Globalization;
using System.Text;
using Disunity.Hosting;
using Disunity.Native;

/// <summary>One record of the frame's work, as the world's encoding carries it. <see cref="Tag"/> is the record's wire tag and
/// <see cref="Line"/> a line of text that describes it. Tests read the fields of the kinds they record: camera renders (0),
/// blits (1), texture copies (2), compute dispatches (3) and pixel reads (12). A blit or copy names its source by an old-style
/// kind (0 none, 1 asset, 2 render texture, 4 temporary render texture) and its target by a kind (0 render texture, 1 temporary).</summary>
internal sealed class WorkRecord
{
    public byte Tag { get; init; }
    public string Line { get; init; } = "";
    /// <summary>A camera render's camera, as <c>slot.generation</c> of the world's component handle.</summary>
    public string Camera { get; init; } = "";
    public uint SourceKind { get; init; }
    public ulong Source { get; init; }
    public uint TargetKind { get; init; }
    /// <summary>The render texture a camera render, blit or copy writes, or the texture a pixel read reads.</summary>
    public ulong Target { get; init; }
    public ulong Material { get; init; }
    public int Pass { get; init; }
    public uint SourceElement { get; init; }
    public uint SourceMip { get; init; }
    public uint TargetElement { get; init; }
    public uint TargetMip { get; init; }
    public ulong Shader { get; init; }
    public uint Kernel { get; init; }
    public uint[] Groups { get; init; } = [];
    public ComputeTestSupport.Entry[] Bindings { get; init; } = [];
    public ulong Request { get; init; }
    public uint X { get; init; }
    public uint Y { get; init; }
    public uint Width { get; init; }
    public uint Height { get; init; }
}

/// <summary>Reads the frame's offscreen work from the graphics snapshot, the way a host takes it, and describes each record as one
/// line of text. The work list is the first field of the snapshot body, so this reads the header and that list and leaves the
/// rest of the snapshot alone.</summary>
internal static unsafe class CommandWireReader
{
    /// <summary>The version of the snapshot encoding this reads; the work list starts the body from this version.</summary>
    private const uint WireVersion = 21;

    /// <summary>Drains the world's offscreen work as one line per record, in call order. A drain also forgets the global writes the
    /// world noted for ordering, so a drain before a step leaves the globals that earlier steps set (and the world's load sent)
    /// out of the step's records.</summary>
    internal static string[] Take() => Work(DisunityHost.WorldHandle).Select(record => record.Line).ToArray();

    /// <summary>Drains the world's offscreen work as records, in call order, through <c>disunity_graphics_snapshot</c>.</summary>
    internal static WorkRecord[] Work(ulong world)
    {
        uint length;
        DisunityHost.ThrowIfError(NativeBindings.disunity_graphics_snapshot(world, null, 0, &length));
        var bytes = new byte[length];
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_graphics_snapshot(world, pointer, length, &length));
        var input = new Cursor(bytes);
        var marker = Encoding.ASCII.GetString(bytes, 0, 4);
        input.Skip(4);
        var version = input.U32();
        if (marker != "DGSN" || version != WireVersion)
            throw new InvalidOperationException($"the snapshot has marker '{marker}' and version {version}, this reader reads DGSN version {WireVersion}");
        input.Skip(4);
        return input.List(() => Read(input)).ToArray();
    }

    /// <summary>The name id of the record <see cref="TakeOrdered"/> appends to the work.</summary>
    private static readonly int MarkerId = UnityEngine.Shader.PropertyToID("_CommandWireReaderMarker");

    /// <summary>Drains the work after appending a record of its own. A direct global write is ordered in front of the next record
    /// and has none to precede when the work ends, so a test that reads the ordered writes of a command buffer asks for them
    /// with a record after it. The marker is not part of the result.</summary>
    internal static string[] TakeOrdered()
    {
        AppendMarker();
        var lines = Take();
        var last = $"Release {MarkerId}";
        if (lines.Length == 0 || lines[^1] != last) throw new InvalidOperationException($"the marker record is missing from the work: {string.Join(" | ", lines)}");
        return lines[..^1];
    }

    /// <summary>Appends one record to the work, so the frame has work and a snapshot of it carries the global state.</summary>
    internal static void AppendMarker()
    {
        var marker = new UnityEngine.Rendering.CommandBuffer();
        marker.ReleaseTemporaryRT(MarkerId);
        UnityEngine.Graphics.ExecuteCommandBuffer(marker);
        marker.Release();
    }

    /// <summary>Reads one record of the work list, whose tag is the first byte.</summary>
    private static WorkRecord Read(Cursor input)
    {
        var tag = input.U8();
        return tag switch
        {
            0 => CameraRender(input),
            1 => Blit(input),
            2 => Copy(input),
            3 => Dispatch(input),
            12 => ReadPixels(input),
            _ => new WorkRecord { Tag = tag, Line = Command(tag, input) },
        };
    }

    private static WorkRecord CameraRender(Cursor input)
    {
        var camera = input.Component();
        var target = input.U64();
        return new WorkRecord { Tag = 0, Line = $"CameraRender camera={camera} target={target}", Camera = camera, Target = target };
    }

    private static WorkRecord Blit(Cursor input)
    {
        var (sourceKind, source) = input.TextureRef();
        var (targetKind, target) = input.WorkTarget();
        var material = input.U8() == 0 ? 0 : input.U64();
        var pass = input.I32();
        return new WorkRecord
        {
            Tag = 1, SourceKind = sourceKind, Source = source, TargetKind = targetKind, Target = target, Material = material, Pass = pass,
            Line = $"Blit source={sourceKind}:{source} target={TargetText(targetKind, target)} material={material} pass={pass}",
        };
    }

    private static WorkRecord Copy(Cursor input)
    {
        var (sourceKind, source) = input.TextureRef();
        var (sourceElement, sourceMip) = (input.U32(), input.U32());
        var (targetKind, target) = input.WorkTarget();
        var (targetElement, targetMip) = (input.U32(), input.U32());
        return new WorkRecord
        {
            Tag = 2, SourceKind = sourceKind, Source = source, SourceElement = sourceElement, SourceMip = sourceMip,
            TargetKind = targetKind, Target = target, TargetElement = targetElement, TargetMip = targetMip,
            Line = $"Copy source={sourceKind}:{source}[{sourceElement},{sourceMip}] target={TargetText(targetKind, target)}[{targetElement},{targetMip}]",
        };
    }

    private static WorkRecord Dispatch(Cursor input)
    {
        var (shader, kernel) = (input.U64(), input.U32());
        var groups = new[] { input.U32(), input.U32(), input.U32() };
        var bindings = input.ComputeBindings();
        return new WorkRecord
        {
            Tag = 3, Shader = shader, Kernel = kernel, Groups = groups, Bindings = bindings,
            Line = $"Dispatch shader={shader} kernel={kernel} groups={string.Join("x", groups)} bindings=[{string.Join(",", bindings.Select(entry => entry.Name))}]",
        };
    }

    private static WorkRecord ReadPixels(Cursor input)
    {
        var (request, texture) = (input.U64(), input.U64());
        var (x, y, width, height) = (input.U32(), input.U32(), input.U32(), input.U32());
        return new WorkRecord
        {
            Tag = 12, Request = request, Target = texture, X = x, Y = y, Width = width, Height = height,
            Line = $"ReadPixels request={request} texture={texture} rect=({x},{y},{width},{height})",
        };
    }

    /// <summary>A blit or copy target: a render texture by its world id, or <c>temp:</c> and the property name id of a temporary.</summary>
    private static string TargetText(uint kind, ulong target) => kind == 1
        ? $"temp:{unchecked((int)(uint)target)}"
        : target.ToString(CultureInfo.InvariantCulture);

    /// <summary>The text of a command record whose tag has been read.</summary>
    private static string Command(byte tag, Cursor input)
    {
        var text = tag switch
        {
            4 => $"SetRenderTarget colors=[{string.Join(",", input.List(input.Target))}] depth={input.Optional(input.Target)} mip={input.U32()} face={input.I32()} slice={input.I32()}",
            5 => $"Clear flags={(input.Bool() ? "C" : "")}{(input.Bool() ? "D" : "")}{(input.Bool() ? "S" : "")} color={input.Floats(4)} depth={input.F32()} stencil={input.U32()}",
            6 => $"SetGlobal {input.Property()}",
            7 => $"Keyword {input.Text()}={input.Bool()}",
            8 => $"Allocate {input.I32()} {input.Texture()}",
            9 => $"Release {input.I32()}",
            11 => $"SetupCamera camera={input.Component()} view={input.Floats(16)} projection={input.Floats(16)} target={input.Target()} buffers={input.Optional(input.Buffers)}",
            10 => $"Cull id={input.U64()} camera={input.Component()} mask={input.U32()} planes=[{string.Join(",", input.List(() => input.Floats(4)))}] options={input.U32()}",
            13 => $"CreateList id={input.U64()} {input.RendererList()}",
            14 => $"DrawList list={input.U64()}",
            15 => $"ViewProjection view={input.Floats(16)} projection={input.Floats(16)}",
            16 => $"DrawMesh mesh={input.U64()} matrix={input.Floats(16)} material={input.U64()} submesh={input.U32()} pass={input.I32()}{Instances(input.U32())} block=[{input.Optional(input.Block)}]",
            17 => $"DrawProcedural matrix={input.Floats(16)} material={input.U64()} pass={input.I32()} topology={input.U8()} vertices={input.U32()} instances={input.U32()}",
            18 => $"Viewport rect={input.Floats(4)}",
            19 => $"GenerateMips texture={input.U64()}",
            20 => $"Scissor rect={input.Floats(4)}",
            21 => "ScissorOff",
            22 => $"DepthBias bias={input.F32().ToString("R", CultureInfo.InvariantCulture)} slope={input.F32().ToString("R", CultureInfo.InvariantCulture)}",
            23 => $"DrawMeshBatch mesh={input.U64()} material={input.U64()} submesh={input.U32()} pass={input.I32()} {Batch(input)}",
            var other => throw new InvalidOperationException($"no command has the tag {other}"),
        };
        return text;
    }

    /// <summary>A batch draw's part after the mesh: the buffer window, the metadata words by name, the visible indexes and the pass tags.</summary>
    private static string Batch(Cursor input)
    {
        var buffer = input.U64();
        var window = $"{input.U64()}+{input.U64()}";
        var metadata = string.Join(",", input.List(() => $"{input.Text()}={input.U32()}"));
        var visible = string.Join(",", input.List(input.U32));
        var tags = string.Join(",", input.List(input.Text));
        return $"buffer={buffer} window={window} metadata=[{metadata}] visible=[{visible}] tags=[{tags}]";
    }

    /// <summary>A mesh draw's instance count, written only when it is not the one instance of a plain draw.</summary>
    private static string Instances(uint count) => count == 1 ? "" : $" instances={count}";

    private sealed class Cursor(byte[] bytes)
    {
        private int _at;

        public void Skip(int count) => _at += count;
        public byte U8() => bytes[_at++];
        public bool Bool() => U8() == 1;
        public string Component() { U64(); var slot = U32(); return $"{slot}.{U32()}"; }
        public uint U32() { var v = BitConverter.ToUInt32(bytes, _at); _at += 4; return v; }
        public int I32() { var v = BitConverter.ToInt32(bytes, _at); _at += 4; return v; }
        public ulong U64() { var v = BitConverter.ToUInt64(bytes, _at); _at += 8; return v; }
        public float F32() { var v = BitConverter.ToSingle(bytes, _at); _at += 4; return v; }
        public float[] Raw(int n) => Enumerable.Range(0, n).Select(_ => F32()).ToArray();
        public string Floats(int n) => "(" + string.Join(" ", Enumerable.Range(0, n).Select(_ => F32().ToString("R", CultureInfo.InvariantCulture))) + ")";
        public string Text() { var n = (int)U32(); var s = Encoding.UTF8.GetString(bytes, _at, n); _at += n; return s; }
        public IEnumerable<T> List<T>(Func<T> read) { var n = (int)U32(); return Enumerable.Range(0, n).Select(_ => read()).ToList(); }
        public string RendererList() => U8() switch
        {
            0 => DrawList(),
            1 => ShadowList(),
            var other => throw new InvalidOperationException($"no renderer list has the tag {other}"),
        };

        private string DrawList()
        {
            var cull = U64();
            var tags = string.Join(",", List(Text));
            var (queueMin, queueMax) = (I32(), I32());
            var (layers, renderingLayers, sorting, perObject) = (U32(), U32(), U32(), U32());
            var material = Optional(() => U64().ToString());
            return $"draw cull={cull} tags=[{tags}] queue={queueMin}..{queueMax} layers={layers} renderingLayers={renderingLayers} sorting={sorting} perObject={perObject} material={material} pass={I32()}";
        }

        private string ShadowList()
        {
            var (cull, light) = (U64(), I32());
            var planes = string.Join(",", List(() => Floats(4)));
            return $"shadow cull={cull} light={light} planes=[{planes}] sphere={Floats(4)} filter={U32()} projection={U32()} layerTest={U8()}";
        }

        public string Optional(Func<string> read) => U8() == 0 ? "-" : read();

        /// <summary>A camera's target buffers: the color render textures in order, then the depth render texture or <c>-</c>.</summary>
        public string Buffers()
        {
            var colors = List(U64);
            return $"[{string.Join(",", colors)}]/{Optional(() => U64().ToString())}";
        }

        public string Target() => U8() switch
        {
            0 => $"Render:{U64()}",
            1 => $"Temp:{I32()}",
            2 => "Camera",
            3 => "None",
            var other => throw new InvalidOperationException($"no target has the tag {other}"),
        };

        public string Property()
        {
            var tag = U8();
            var name = Text();
            return tag switch
            {
                0 => $"{name}=float {Floats(1)}",
                1 => $"{name}=int {I32()}",
                2 => $"{name}=vector {Floats(4)}",
                3 => $"{name}=matrix {Floats(16)}",
                4 => $"{name}=texture {Slot()}",
                5 => $"{name}=vectors {Elements(4)}",
                6 => $"{name}=matrices {Elements(16)}",
                var other => throw new InvalidOperationException($"no property has the tag {other}"),
            };
        }

        /// <summary>A property block as the world writes one: floats, ints, vectors, matrices, textures, then vector and matrix
        /// arrays, each list sorted by name. Values print as <c>name=kind value</c>, joined by ", ".</summary>
        public string Block()
        {
            var parts = new List<string>();
            parts.AddRange(List(() => $"{Text()}=float {Floats(1)}"));
            parts.AddRange(List(() => $"{Text()}=int {I32()}"));
            parts.AddRange(List(() => $"{Text()}=vector {Floats(4)}"));
            parts.AddRange(List(() => $"{Text()}=matrix {Floats(16)}"));
            parts.AddRange(List(() => $"{Text()}=texture {Slot()}"));
            parts.AddRange(List(() => $"{Text()}=vectors {Elements(4)}"));
            parts.AddRange(List(() => $"{Text()}=matrices {Elements(16)}"));
            return string.Join(", ", parts);
        }

        /// <summary>The count of an array and its elements of <paramref name="width"/> floats each.</summary>
        private string Elements(int width)
        {
            var count = (int)U32();
            return $"[{count}] " + string.Join(" ", Enumerable.Range(0, count).Select(_ => Floats(width)));
        }

        private string Slot()
        {
            var kind = U8();
            var texture = kind switch { 0 => $"asset:{U64()}", 1 => $"render:{U64()}", 2 => $"global:{Text()}", 4 => $"temporary:{I32()}", _ => "none" };
            return $"{texture} scale={Floats(2)} offset={Floats(2)}";
        }

        /// <summary>A texture reference of recorded work as the kind a flat record gave it (0 none, 1 asset, 2 render texture, 4 temporary
        /// render texture) and its identity: the world's id, or the property name id of a temporary as its low 32 bits.</summary>
        public (uint Kind, ulong Id) TextureRef() => U8() switch
        {
            0 => (1, U64()),
            1 => (2, U64()),
            3 or 5 => (0, 0),
            4 => (4, unchecked((uint)I32())),
            var other => throw new InvalidOperationException($"recorded work has no texture reference with the tag {other}"),
        };

        /// <summary>The target of a blit or copy: 0 and a render texture's world id, or 1 and the property name id of a temporary.
        /// The world records no other target there, and the camera target and none read as render texture 0.</summary>
        public (uint Kind, ulong Id) WorkTarget() => U8() switch
        {
            0 => (0, U64()),
            1 => (1, unchecked((uint)I32())),
            2 or 3 => (0, 0),
            var other => throw new InvalidOperationException($"no target has the tag {other}"),
        };

        /// <summary>The values a dispatch ran with: properties as <see cref="Block"/> lays them out, then the buffers by name. Entries
        /// are floats (kind 0), integers (1), vectors (2), matrices (3), textures (4), vector arrays (6) and matrix arrays (7), then
        /// the buffers (5), each group sorted by name. An array entry holds the floats of its elements one after another.</summary>
        public ComputeTestSupport.Entry[] ComputeBindings()
        {
            var entries = new List<ComputeTestSupport.Entry>();
            entries.AddRange(List(() => Value(Text(), 0, [F32()])));
            entries.AddRange(List(() => Value(Text(), 1, [], I32())));
            entries.AddRange(List(() => Value(Text(), 2, Raw(4))));
            entries.AddRange(List(() => Value(Text(), 3, Raw(16))));
            entries.AddRange(List(TextureValue));
            entries.AddRange(List(() => Value(Text(), 6, Arrays(4))));
            entries.AddRange(List(() => Value(Text(), 7, Arrays(16))));
            entries.AddRange(List(() => new ComputeTestSupport.Entry(Text(), 5, 0, U64(), 0, [])));
            return [.. entries];
        }

        private static ComputeTestSupport.Entry Value(string name, uint kind, float[] floats, int value = 0) =>
            new(name, kind, 0, 0, value, floats);

        private float[] Arrays(int width) => Raw((int)U32() * width);

        private ComputeTestSupport.Entry TextureValue()
        {
            var name = Text();
            var (kind, id) = TextureRef();
            return new(name, 4, kind, id, 0, [F32(), F32(), F32(), F32()]);
        }

        public string Texture()
        {
            var (w, h, format, depth, mips) = (U32(), U32(), U32(), U32(), U32());
            var srgb = Bool();
            var (dimension, volumeDepth) = (U32(), U32());
            var autoMips = Bool() ? " automips" : "";
            var (wu, wv, ww, filter, aniso, bias) = (U32(), U32(), U32(), U32(), U32(), F32());
            U64();
            var shape = dimension == 2 && volumeDepth == 1 ? "" : $" shape={dimension}x{volumeDepth}";
            return $"{w}x{h} format={format} depth={depth} mips={mips} srgb={srgb}{shape}{autoMips} wrap={wu}{wv}{ww} filter={filter} aniso={aniso} bias={bias}";
        }
    }
}
