using System.Reflection;
using System.Reflection.Metadata;
using System.Reflection.Metadata.Ecma335;
using Disunity.Hosting;

internal static class PlayerApiReportTests
{
    public static void Run()
    {
        AcceptsExactSignatures();
        ReportsWrongParameterType();
        ReportsWrongReturnType();
        ReportsWrongFieldType();
        ReportsWrongGenericArity();
        ReportsWrongInstanceKind();
        ReportsWrongConstructorType();
        ChecksGenericDeclaringTypes();
        ReportsDistinctOverloadsAndUsers();
        ReportsWrongElementShapes();
        Console.WriteLine("PASS: player API report matches member signatures and generic declaring types");
    }

    public static void AcceptsExactSignatures()
    {
        var report = Report((game, type) =>
        {
            game.MemberRef(type, "Take", Method(true, 0, r => r.Void(), p => p.Type().Int32()));
            game.MemberRef(type, "Return", Method(true, 0, r => r.Type().Int32(), p => p.Type().Int32()));
            game.MemberRef(type, ".ctor", Method(true, 0, r => r.Void(), p => p.Type().Int32()));
            game.MemberRef(type, "Static", Method(false, 0, r => r.Void(), p => p.Type().Int32()));
            game.MemberRef(type, "Echo", Method(true, 1, r => r.Type().GenericMethodTypeParameter(0),
                p => p.Type().GenericMethodTypeParameter(0)));
            game.MemberRef(type, "Change", Method(true, 0, r => r.Void(), p => p.Type(isByRef: true).Int32()));
            game.MemberRef(type, "Vector", Method(true, 0, r => r.Void(), p => p.Type().SZArray().Int32()));
            game.FieldRef(type, "Number");
        });
        Require(report.Count == 0, "exact signatures must match: " + Describe(report));
    }

    public static void ReportsWrongParameterType() =>
        Missing("Take", Method(true, 0, r => r.Void(), p => p.Type().Boolean()));

    public static void ReportsWrongReturnType() =>
        Missing("Return", Method(true, 0, r => r.Type().Boolean(), p => p.Type().Int32()));

    public static void ReportsWrongFieldType()
    {
        var signature = new BlobBuilder();
        new BlobEncoder(signature).FieldSignature().Boolean();
        var report = Report((game, type) => game.MemberRef(type, "Number", signature));
        Require(report.Count == 1 && report[0].Member!.Contains("Number"), "wrong field type: " + Describe(report));
    }

    public static void ReportsWrongGenericArity() =>
        Missing("Echo", Method(true, 0, r => r.Type().Int32(), p => p.Type().Int32()));

    public static void ReportsWrongInstanceKind() =>
        Missing("Static", Method(true, 0, r => r.Void(), p => p.Type().Int32()));

    public static void ReportsWrongConstructorType() =>
        Missing(".ctor", Method(true, 0, r => r.Void(), p => p.Type().Boolean()));

    public static void ChecksGenericDeclaringTypes()
    {
        var report = Report((game, _) =>
        {
            var type = game.TypeRef(game.Reference("UnityEngine.TestModule"), "PlayerReportFixture", "GenericApi`1");
            var parent = game.TypeSpec(e => e.GenericInstantiation(type, 1, isValueType: false).AddArgument().Int32());
            game.MemberRef(parent, "Take", Method(true, 0, r => r.Void(), p => p.Type().GenericTypeParameter(0)));
            var field = new BlobBuilder();
            new BlobEncoder(field).FieldSignature().GenericTypeParameter(0);
            game.MemberRef(parent, "Value", field);
            game.MemberRef(parent, "Missing", Method(true, 0, r => r.Void()));
        });
        Require(report.Count == 1 && report[0].Member!.Contains("Missing"),
            "generic declaring type must accept its real members and report its missing member: " + Describe(report));
    }

    public static void ReportsDistinctOverloadsAndUsers()
    {
        var game = Game((image, type) =>
        {
            image.MemberRef(type, "Take", Method(true, 0, r => r.Void(), p => p.Type().String()));
            image.MemberRef(type, "Take", Method(true, 0, r => r.Void(), p => p.Type().Boolean()));
        });
        using var first = new MemoryStream(game);
        using var second = new MemoryStream(game);
        var report = PlayerApiReport.Build(new[] { ("Z", (Stream)first), ("A", (Stream)second) }, typeof(PlayerReportFixture.Api).Assembly);
        Require(report.Count == 2 && report.All(m => m.ReferencedBy.SequenceEqual(new[] { "A", "Z" })),
            "same-arity overloads remain distinct and users are sorted: " + Describe(report));
        Require(string.CompareOrdinal(report[0].Member, report[1].Member) < 0, "missing overloads are sorted ordinally");
        Require(report[0].Member!.Contains("System.Boolean") && report[1].Member!.Contains("System.String"),
            "missing overloads identify their parameter types: " + Describe(report));
    }

    public static void ReportsWrongElementShapes()
    {
        Missing("Change", Method(true, 0, r => r.Void(), p => p.Type().Int32()));
        Missing("Vector", Method(true, 0, r => r.Void(), p => p.Type().SZArray().Boolean()));
    }

    private static void Missing(string member, BlobBuilder signature)
    {
        var report = Report((game, type) => game.MemberRef(type, member, signature));
        Require(report.Count == 1 && report[0].Member!.Contains(member), "missing " + member + ": " + Describe(report));
    }

    private static IReadOnlyList<MissingApi> Report(Action<SyntheticAssembly, TypeReferenceHandle> references)
    {
        using var stream = new MemoryStream(Game(references));
        return PlayerApiReport.Build(new[] { ("Synthetic", (Stream)stream) }, typeof(PlayerReportFixture.Api).Assembly);
    }

    private static byte[] Game(Action<SyntheticAssembly, TypeReferenceHandle> references)
    {
        var game = new SyntheticAssembly("ReportConsumer");
        var type = game.TypeRef(game.Reference("UnityEngine.TestModule"), "PlayerReportFixture", "Api");
        references(game, type);
        return game.Build();
    }

    private static BlobBuilder Method(bool instance, int genericCount, Action<ReturnTypeEncoder> result,
        params Action<ParameterTypeEncoder>[] parameters)
    {
        var signature = new BlobBuilder();
        new BlobEncoder(signature).MethodSignature(genericParameterCount: genericCount, isInstanceMethod: instance)
            .Parameters(parameters.Length, result, list =>
            {
                foreach (var parameter in parameters) parameter(list.AddParameter());
            });
        return signature;
    }

    private static string Describe(IEnumerable<MissingApi> report) => string.Join("; ", report.Select(m => m.Type + " " + m.Member));

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}

namespace PlayerReportFixture
{
    public class Api
    {
        public int Number;
        public Api(int value) => Number = value;
        public void Take(int value) { }
        public int Return(int value) => value;
        public static void Static(int value) { }
        public U Echo<U>(U value) => value;
        public void Change(ref int value) { }
        public void Vector(int[] value) { }
    }

    public class GenericApi<T>
    {
        public T Value = default!;
        public void Take(T value) { }
    }
}
