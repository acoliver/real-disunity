using System.Reflection;
using System.Runtime.InteropServices;
using Disunity;

var assembly = typeof(WorldRunner).Assembly;
Type AbiType(string name) => assembly.GetType($"Disunity.{name}", throwOnError: true)!;
var bindings = AbiType("NativeBindings");
var result = AbiType("DisunityResult");
var command = AbiType("DisunityCommand");
var state = AbiType("DisunityObjectState");
var u32 = typeof(uint);
var u64 = typeof(ulong);
var f32 = typeof(float);
const BindingFlags Imports = BindingFlags.Static | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

void Require(bool condition, string message)
{
    if (!condition) { throw new InvalidOperationException(message); }
}

void CheckImport(string name, params Type[] parameters)
{
    var method = bindings.GetMethod(name, Imports)
        ?? throw new InvalidOperationException($"missing import {name}");
    var import = method.GetCustomAttribute<DllImportAttribute>()
        ?? throw new InvalidOperationException($"missing DllImport {name}");
    Require(method.ReturnType == result, $"{name}: wrong result type");
    Require(method.GetParameters().Select(p => p.ParameterType).SequenceEqual(parameters),
        $"{name}: wrong complete parameter signature");
    Require(import.Value == "worldrunner_disunity_ffi", $"{name}: wrong native image");
    Require(import.EntryPoint == name && import.ExactSpelling, $"{name}: wrong entry point");
    Require(import.CallingConvention == CallingConvention.Cdecl, $"{name}: wrong calling convention");
}

void CheckLayout(Type type, int size, params (string Name, Type Type, int Offset)[] fields)
{
    Require(type.IsLayoutSequential, $"{type}: layout is not sequential");
    Require(Marshal.SizeOf(type) == size, $"{type}: wrong size");
    Require(type.GetFields(BindingFlags.Instance | BindingFlags.Public).Length == fields.Length,
        $"{type}: unexpected field count");
    foreach (var field in fields)
    {
        Require(type.GetField(field.Name)?.FieldType == field.Type, $"{type}.{field.Name}: wrong field type");
        Require(Marshal.OffsetOf(type, field.Name).ToInt32() == field.Offset,
            $"{type}.{field.Name}: wrong offset");
    }
}

CheckImport("disunity_abi_version", u32.MakePointerType());
CheckImport("disunity_world_create", u64.MakePointerType());
CheckImport("disunity_world_destroy", u64);
CheckImport("disunity_gameobject_create", u64, u64.MakePointerType());
CheckImport("disunity_gameobject_destroy", u64, u64);
CheckImport("disunity_transform_set_position", u64, u64, f32, f32, f32);
CheckImport("disunity_transform_set_rotation_y", u64, u64, f32);
CheckImport("disunity_command_buffer_submit", u64, command.MakePointerType(), u32);
CheckImport("disunity_world_step", u64, f32, u64);
CheckImport("disunity_error_message", u64, typeof(byte).MakePointerType(), u32, u32.MakePointerType());
CheckImport("disunity_store_statistics", u64.MakePointerType(), u64.MakePointerType());
CheckImport("disunity_world_export_objects", u64, state.MakePointerType(), u32, u32.MakePointerType());
Require(bindings.GetMethods(Imports).Length == 12, "unexpected native import count");

CheckLayout(result, 16, ("code", typeof(int), 0), ("detail", u64, 8));
CheckLayout(command, 24, ("object", u64, 0), ("kind", u32, 8),
    ("x", f32, 12), ("y", f32, 16), ("z", f32, 20));
CheckLayout(state, 32, ("object", u64, 0), ("x", f32, 8), ("y", f32, 12),
    ("z", f32, 16), ("rotation_y_radians", f32, 20), ("material_slot", u32, 24));
Console.WriteLine("PASS: all 12 native import signatures/calling conventions and all 3 ABI struct layouts");
