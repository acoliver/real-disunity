using System.Globalization;
using System.Text;
using Disunity.Hosting;
using Disunity.Native;

/// <summary>Reads the offscreen work a test took from the world and describes each record as one line of text. The command
/// records carry their parts in the bytes of <c>disunity_offscreen_detail</c>, which this reads the way a host decodes the
/// world's record encoding.</summary>
internal static unsafe class CommandWireReader
{
    /// <summary>Drains the world's offscreen work as one line per record, in call order.</summary>
    internal static string[] Take()
    {
        var world = DisunityHost.WorldHandle;
        uint count;
        DisunityHost.ThrowIfError(NativeBindings.disunity_offscreen_take(world, null, 0, &count));
        var work = new DisunityOffscreenWork[count];
        fixed (DisunityOffscreenWork* pointer = work)
            DisunityHost.ThrowIfError(NativeBindings.disunity_offscreen_take(world, count == 0 ? null : pointer, count, &count));
        return work.Select(record => record.kind >= 4 ? Describe(Detail(world, record.bindings)) : Flat(record)).ToArray();
    }

    private static string Flat(DisunityOffscreenWork r) => r.kind switch
    {
        1 => $"Blit source={r.source_kind}:{r.source} target={r.target} material={r.material} pass={r.pass}",
        2 => $"Copy source={r.source_kind}:{r.source}[{r.source_element},{r.source_mip}] target={r.target}[{r.target_element},{r.target_mip}]",
        var other => $"Flat{other}",
    };

    private static byte[] Detail(ulong world, uint token)
    {
        uint length;
        DisunityHost.ThrowIfError(NativeBindings.disunity_offscreen_detail(world, token, null, 0, &length));
        var bytes = new byte[length];
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_offscreen_detail(world, token, pointer, length, &length));
        return bytes;
    }

    private static string Describe(byte[] bytes)
    {
        var input = new Cursor(bytes);
        var text = input.U8() switch
        {
            4 => $"SetRenderTarget colors=[{string.Join(",", input.List(input.Target))}] depth={input.Optional(input.Target)} mip={input.U32()} face={input.I32()} slice={input.I32()}",
            5 => $"Clear flags={(input.Bool() ? "C" : "")}{(input.Bool() ? "D" : "")}{(input.Bool() ? "S" : "")} color={input.Floats(4)} depth={input.F32()} stencil={input.U32()}",
            6 => $"SetGlobal {input.Property()}",
            7 => $"Keyword {input.Text()}={input.Bool()}",
            8 => $"Allocate {input.I32()} {input.Texture()}",
            9 => $"Release {input.I32()}",
            11 => $"SetupCamera camera={input.Component()} view={input.Floats(16)} projection={input.Floats(16)} target={input.Target()}",
            10 => $"Cull id={input.U64()} camera={input.Component()} mask={input.U32()} planes=[{string.Join(",", input.List(() => input.Floats(4)))}] options={input.U32()}",
            13 => $"CreateList id={input.U64()} {input.RendererList()}",
            14 => $"DrawList list={input.U64()}",
            15 => $"ViewProjection view={input.Floats(16)} projection={input.Floats(16)}",
            16 => $"DrawMesh mesh={input.U64()} matrix={input.Floats(16)} material={input.U64()} submesh={input.U32()} pass={input.I32()}",
            var other => throw new InvalidOperationException($"no command has the tag {other}"),
        };
        if (input.Remaining != 0) throw new InvalidOperationException($"{input.Remaining} bytes follow the record '{text}'");
        return text;
    }

    private sealed class Cursor(byte[] bytes)
    {
        private int _at;

        public int Remaining => bytes.Length - _at;
        public byte U8() => bytes[_at++];
        public bool Bool() => U8() == 1;
        public string Component() { U64(); var slot = U32(); return $"{slot}.{U32()}"; }
        public uint U32() { var v = BitConverter.ToUInt32(bytes, _at); _at += 4; return v; }
        public int I32() { var v = BitConverter.ToInt32(bytes, _at); _at += 4; return v; }
        public ulong U64() { var v = BitConverter.ToUInt64(bytes, _at); _at += 8; return v; }
        public float F32() { var v = BitConverter.ToSingle(bytes, _at); _at += 4; return v; }
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

        /// <summary>The count of an array and its elements of <paramref name="width"/> floats each.</summary>
        private string Elements(int width)
        {
            var count = (int)U32();
            return $"[{count}] " + string.Join(" ", Enumerable.Range(0, count).Select(_ => Floats(width)));
        }

        private string Slot()
        {
            var kind = U8();
            var texture = kind switch { 0 => $"asset:{U64()}", 1 => $"render:{U64()}", 2 => $"global:{Text()}", _ => "none" };
            return $"{texture} scale={Floats(2)} offset={Floats(2)}";
        }

        public string Texture()
        {
            var (w, h, format, depth, mips) = (U32(), U32(), U32(), U32(), U32());
            var srgb = Bool();
            var (dimension, volumeDepth) = (U32(), U32());
            var (wu, wv, ww, filter, aniso, bias) = (U32(), U32(), U32(), U32(), U32(), F32());
            U64();
            var shape = dimension == 2 && volumeDepth == 1 ? "" : $" shape={dimension}x{volumeDepth}";
            return $"{w}x{h} format={format} depth={depth} mips={mips} srgb={srgb}{shape} wrap={wu}{wv}{ww} filter={filter} aniso={aniso} bias={bias}";
        }
    }
}
