using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

static void Require(bool condition, string message)
{
    if (!condition) throw new InvalidOperationException(message);
}

TestArtifacts.Configure(ref args);
var world = new RecordingWorld("{\"version\":1,\"ok\":true,\"data\":{\"value\":9}}");
NativeWorld.Attach(world);
using (var response = NativeWorld.Request("time.get"))
{
    Require(NativeWorld.Data(response).GetProperty("value").GetInt32() == 9,
        "native response data was not returned");
}
Require(world.LastRequest == "{\"version\":1,\"op\":\"time.get\"}",
    "request must carry protocol version and operation");

var failing = new RecordingWorld("{\"version\":1,\"ok\":false,\"error\":{\"code\":\"unsupported\",\"message\":\"time.unavailable\"}}");
NativeWorld.Attach(failing);
try
{
    using var ignored = NativeWorld.Request("time.get");
    throw new InvalidOperationException("native semantic failure was hidden");
}
catch (NativeWorldException exception)
{
    Require(exception.Code == "unsupported" && exception.Message.Contains("time.unavailable"),
        "native failure context was not preserved");
}
NativeWorld.Detach();
BridgeBehavior.BeforeLoad();
if (args.Contains("--contextual-metadata-only")) { ContextualMetadataBehavior.Run(); return; }
if (args.Contains("--contextual-bounded-codec-only")) { ContextualCleanupCodecBehavior.Run(); return; }
if (args.Contains("--contextual-codec-only")) { ContextualCodecBehavior.Run(); return; }
if (args.Contains("--contextual-cleanup-only")) { ContextualCleanupBehavior.Run(TestArtifacts.Native, args.Contains("--mutation")); return; }
if (args.Contains("--nested-movenext-only")) { NestedStartBehavior.Run(TestArtifacts.Native, true); return; }
if (args.Contains("--nested-scenario")) { NestedScenarioBehavior.Run(TestArtifacts.Native, int.Parse(args[1]), args.Length > 2 ? int.Parse(args[2]) : 1); return; }
if (args.Contains("--nested-authored-breakout")) { NestedOriginalBehavior.Breakout(TestArtifacts.Native, TestArtifacts.Repository, true); return; }
if (args.Contains("--nested-original-breakout")) { NestedOriginalBehavior.Breakout(TestArtifacts.Native, TestArtifacts.Repository); return; }
if (args.Contains("--nested-unsupported")) { NestedSelectionBehavior.Unsupported(TestArtifacts.Native); return; }
if (args.Contains("--nested-matching")) { NestedSelectionBehavior.Matching(TestArtifacts.Native); return; }
if (args.Contains("--nested-authorization")) { ReadinessAuthorization.Run(TestArtifacts.Native, true); return; }
if (args.Contains("--nested-waits")) { ReadinessWaitBehavior.Run(TestArtifacts.Native, true); return; }
if (args.Contains("--nested-original-pong")) { NestedPongBehavior.Run(TestArtifacts.Native, TestArtifacts.Repository, true); return; }
if (args.Contains("--single-original-pong")) { NestedPongBehavior.Run(TestArtifacts.Native, TestArtifacts.Repository, false); return; }
if (args.Contains("--nested-start-only")) { NestedStartBehavior.Run(TestArtifacts.Native); return; }
if (args.Contains("--byte-profile-missing-requirement")) { ByteProfileBehavior.RejectMissingRequirement(TestArtifacts.Native); return; }
if (args.Contains("--byte-profile-boundaries")) { ByteProfileBoundaries.Run(TestArtifacts.Native); return; }
if (args.Contains("--byte-profile-copy-fault")) { ByteCopyFault.Run(TestArtifacts.Native); return; }
if (args.Contains("--byte-profile-descriptor")) { ByteProfileBehavior.Descriptor(); return; }
if (args.Contains("--byte-profile-only")) { ByteProfileBehavior.Run(TestArtifacts.Native); return; }
if (args.Contains("--byte-profile-unselected")) { ByteProfileBehavior.Unselected(TestArtifacts.Native); return; }
if (args.Contains("--readiness-only"))
{
    var readinessPath = TestArtifacts.Native;
    ReadinessBehavior.Run(readinessPath);
    ReadinessWaitBehavior.Run(readinessPath);
    ReadinessWaitBehavior.Unsupported(readinessPath);
    ReadinessContactBehavior.Run(readinessPath);
    ReadinessLifecycleBehavior.Exceptions(readinessPath);
    ReadinessLifecycleBehavior.Mutation(readinessPath);
    ReadinessLifecycleBehavior.Clone(readinessPath);
    ReadinessAuthorization.Run(readinessPath);
    ReadinessTickets.Run(readinessPath);
    ReadinessCurrentCancellation.Run(readinessPath);
    ReadinessWaitBehavior.IteratorFailures(readinessPath);
    ReadinessCleanupBehavior.Run(readinessPath);
    ReadinessStartAdmission.Run(readinessPath);
    return;
}
if (args.Contains("--readiness-missing-provider"))
{
    ReadinessAuthorization.MissingProvider(TestArtifacts.Native);
    return;
}
if (args.Contains("--handshake-describe-only")) { HandshakeBehavior.Describe(); return; }
if (args.Contains("--handshake-load-only"))
{
    HandshakeRunner.Run();
    return;
}
HandshakeBehavior.Describe();
var exports = typeof(ManagedBridge).GetMethods(BindingFlags.Public | BindingFlags.Static)
    .Select(method => (Method: method, Attribute: method.GetCustomAttribute<UnmanagedCallersOnlyAttribute>()))
    .Where(item => item.Attribute is not null).ToArray();
var expectedExports = new[] { "ConfigureNativeLibrary", "LoadFixture", "DispatchPhase", "InvokeContact", "Observe", "GetLastError", "Unload", "DescribeBridge" };
Require(exports.Length == expectedExports.Length, "managed bridge export count differs from host contract");
foreach (var name in expectedExports)
{
    var export = exports.SingleOrDefault(item => item.Attribute!.EntryPoint == name);
    var method = export.Method ?? throw new InvalidOperationException($"missing Cdecl bridge export {name}");
    Require(method.ReturnType == typeof(int), $"{name} must return i32");
    Require(export.Attribute!.CallConvs!.Contains(typeof(CallConvCdecl)), $"{name} must use Cdecl");
}
Require(exports.Single(item => item.Attribute!.EntryPoint == "Unload").Method.GetParameters().Length == 0,
    "Unload must take no parameters");

var repository = TestArtifacts.Repository;
OriginalAssemblyBehavior.Verify(repository);
var nativePath = TestArtifacts.Native;
var nativeImage = System.Runtime.InteropServices.NativeLibrary.Load(nativePath);
try
{
    var create = Marshal.GetDelegateForFunctionPointer<WorldCreate>(
        System.Runtime.InteropServices.NativeLibrary.GetExport(nativeImage, "disunity_world_create"));
    var destroy = Marshal.GetDelegateForFunctionPointer<WorldDestroy>(
        System.Runtime.InteropServices.NativeLibrary.GetExport(nativeImage, "disunity_world_destroy"));
    void RunIsolated(Action action)
    {
        Require(create(out var isolated).code == 0, "native test world creation failed");
        NativeWorld.AttachNativeLibrary(nativePath, isolated);
        try { action(); }
        finally { NativeWorld.Detach(); Require(destroy(isolated).code == 0, "native test world teardown failed"); }
    }
    if (args.Contains("--callbacks-only"))
    {
        Require(create(out var callbackWorld).code == 0, "callback world creation");
        try { CallbackIsolationBehavior.Run(nativePath, callbackWorld); CallbackFailureBudgetBehavior.Run(nativePath, callbackWorld); }
        finally { NativeWorld.Detach(); Require(destroy(callbackWorld).code == 0, "callback world teardown"); }
        RunIsolated(CallbackIsolationBehavior.Contact);
        RunIsolated(CallbackStartBehavior.Run);
        return;
    }
    if (args.Contains("--lifecycle-budget-only")) { RunIsolated(LifecycleReadBudgetBehavior.Run); return; }
    if (args.Contains("--lifecycle-cleanup-only")) { RunIsolated(LifecycleCleanupMutationBehavior.Run); return; }
    RunIsolated(LifecycleReadBudgetBehavior.Run);
    RunIsolated(LifecycleCleanupMutationBehavior.Run);
    DisposalBehavior.Run(nativePath, nativeImage);
    if (args.Contains("--disposal-only")) return;
    Require(create(out var isolationWorld).code == 0, "callback world creation");
    try { CallbackIsolationBehavior.Run(nativePath, isolationWorld); CallbackFailureBudgetBehavior.Run(nativePath, isolationWorld); }
    finally { NativeWorld.Detach(); Require(destroy(isolationWorld).code == 0, "callback world teardown"); }
    RunIsolated(CallbackIsolationBehavior.Contact);
    RunIsolated(CallbackStartBehavior.Run);
    RunIsolated(() => OriginalDeathZoneBehavior.Run(repository));
    RunIsolated(NativeEligibilityBehavior.Liveness);
    RunIsolated(NativeEligibilityBehavior.Lifecycle);
    RunIsolated(BinderBehavior.Run);
    RunIsolated(() => OriginalGameBehavior.Snake(repository));
    RunIsolated(() => OriginalGameBehavior.Snake(repository, "compat-2017.1"));
    RunIsolated(() => OriginalGameBehavior.Breakout(repository));
    RunIsolated(() => PongConstructedBehavior.Run(repository));
    RunIsolated(UiFacadeBehavior.Run);
    RunIsolated(ContactBehavior.Run);
    RunIsolated(ContactLifetimeBehavior.QueuedExit);
    RunIsolated(ContactLifetimeBehavior.RemainingRecipients);
    RunIsolated(ContactLifetimeBehavior.ColliderRemoval);
    RunIsolated(ContactLifetimeBehavior.CallbackFailure);
    RunIsolated(TileColliderBehavior.Run);
    RunIsolated(CompoundContactBehavior.Run);
    RunIsolated(CoroutineBehavior.Run);
    RunIsolated(CloneBehavior.Run);
    RunIsolated(ActivationBehavior.Run);
    RunIsolated(LifecycleMutationBehavior.Run);
    foreach (var phase in new[] { "Start", "FixedUpdate", "Update", "LateUpdate" })
        {
            RunIsolated(() => LifecycleMutationBehavior.Phase(phase));
            RunIsolated(() => LifecycleMutationBehavior.Phase(phase, true));
        }
    RunIsolated(SerializationBehavior.Run);
    RunIsolated(PublicObservationBehavior.Run);
    RunIsolated(IdentityLifetimeBehavior.Run);
    Require(create(out var bridgeWorld).code == 0, "bridge world creation failed");
    NativeWorld.AttachNativeLibrary(nativePath, bridgeWorld);
    try { BridgeBehavior.Run(repository, nativePath, bridgeWorld); }
    finally { NativeWorld.Detach(); Require(destroy(bridgeWorld).code == 0, "bridge world teardown failed"); }
    RunIsolated(() => OriginalGameBehavior.Pong(repository));
    RunIsolated(() => OriginalGameBehavior.Pong(repository, "compat-6000.3"));
    Require(create(out var nativeWorld).code == 0, "native world creation failed");
    try
    {
        NativeWorld.AttachNativeLibrary(nativePath, nativeWorld);
        using var created = NativeWorld.Request("object.create", ("name", "managed-integration"));
        var objectHandle = NativeWorld.Data(created).GetProperty("handle").GetUInt64();
        NativeWorld.Request("transform.set", ("object", objectHandle), ("position", new[] { 12f, 7f, 0f }), ("space", "world")).Dispose();
        using var moved = NativeWorld.Request("transform.get", ("object", objectHandle));
        var position = NativeWorld.Data(moved).GetProperty("position");
        Require(position[0].GetSingle() == 12f && position[1].GetSingle() == 7f,
            "managed transform request did not update native state");
        using var clone = NativeWorld.Request("object.clone", ("object", objectHandle));
        Require(NativeWorld.Data(clone).GetProperty("object").GetProperty("handle").GetUInt64() != objectHandle,
            "native clone did not produce a distinct handle");
        NativeWorld.Request("input.set", ("keys", new[] { "W" }), ("axes", new Dictionary<string, float>())).Dispose();
        Require(Input.GetKeyDown(KeyCode.W), "native input edge was not observed through the managed facade");
        NativeWorld.Request("frame.begin", ("dt", 0.125f)).Dispose();
        Require(MathF.Abs(Time.deltaTime - 0.125f) < 1e-6f, "native time was not observed through the managed facade");
    }
    finally
    {
        NativeWorld.Detach();
        Require(destroy(nativeWorld).code == 0, "native world teardown failed");
    }
}
finally
{
    System.Runtime.InteropServices.NativeLibrary.Free(nativeImage);
}
Console.WriteLine("PASS: native JSON transport, native-backed transform/clone/input/time, and bridge ABI contract");

[StructLayout(LayoutKind.Sequential)]
struct NativeResult
{
    public int code;
    public ulong detail;
}

[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
delegate NativeResult WorldCreate(out ulong world);
[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
delegate NativeResult WorldDestroy(ulong world);

sealed class RecordingWorld(string response) : INativeWorld
{
    public string? LastRequest { get; private set; }
    public JsonDocument Request(JsonDocument request)
    {
        LastRequest = request.RootElement.GetRawText();
        return JsonDocument.Parse(response);
    }
    public double TimeValue(string field) => throw new NotSupportedException(field);
}
