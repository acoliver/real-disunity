using System.Reflection;
using System.Runtime.InteropServices;
using UnityEngine;
using Disunity.Hosting;

var assembly = typeof(DisunityHost).Assembly;
Type AbiType(string name) => assembly.GetType($"Disunity.Native.{name}", throwOnError: true)!;
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

var checkedImports = new HashSet<string>();

void CheckImport(string name, params Type[] parameters)
{
    checkedImports.Add(name);
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
CheckImport("disunity_gameobject_destroy_after", u64, u64, u64, u64);
CheckImport("disunity_transform_set_position", u64, u64, f32, f32, f32);
CheckImport("disunity_transform_set_rotation_y", u64, u64, f32);
CheckImport("disunity_command_buffer_submit", u64, command.MakePointerType(), u32);

CheckImport("disunity_error_message", u64, typeof(byte).MakePointerType(), u32, u32.MakePointerType());
CheckImport("disunity_store_statistics", u64.MakePointerType(), u64.MakePointerType());
CheckImport("disunity_world_export_objects", u64, state.MakePointerType(), u32, u32.MakePointerType());
var advance = AbiType("DisunityAdvance");
var clock = AbiType("DisunityClock");
var buffer = AbiType("DisunityBuffer");
var bytes = typeof(byte).MakePointerType();
CheckImport("disunity_abi_check", u32);
CheckImport("disunity_script_register", u64, u64, u64, u64.MakePointerType());
CheckImport("disunity_object_wake", u64, u64, advance.MakePointerType());
CheckImport("disunity_construction_begin", u64, u64.MakePointerType());
CheckImport("disunity_construction_object_create", u64, u64, u64.MakePointerType());
CheckImport("disunity_construction_activate", u64, u64, advance.MakePointerType());
CheckImport("disunity_construction_abort", u64, u64);
CheckImport("disunity_world_advance", u64, u64, u64, advance.MakePointerType());
CheckImport("disunity_world_resume", u64, u64, u32, bytes, u32, advance.MakePointerType());
CheckImport("disunity_world_clock", u64, clock.MakePointerType());
CheckImport("disunity_error_take", u64, buffer.MakePointerType());
CheckImport("disunity_buffer_copy", u64, bytes, u32, u32.MakePointerType());
CheckImport("disunity_buffer_free", u64);
CheckImport("disunity_gameobject_read", u64, u64, state.MakePointerType());
CheckImport("disunity_clone_create", u64, u64, u64, u64.MakePointerType(), u32.MakePointerType());
CheckImport("disunity_clone_copy", u64, u64, AbiType("DisunityCloneRow").MakePointerType(), u32, u32.MakePointerType());
CheckImport("disunity_clone_bind", u64, u64, u64, u64);
CheckImport("disunity_transform_parent", u64, u64, u64, u32, advance.MakePointerType());
CheckImport("disunity_transform_get_parent", u64, u64, u64.MakePointerType());
CheckImport("disunity_gameobject_set_active", u64, u64, u32, advance.MakePointerType());
CheckImport("disunity_clone_abort", u64, u64);
CheckImport("disunity_clone_activate", u64, u64, advance.MakePointerType());
CheckImport("disunity_callback_read", u64, u64, AbiType("DisunityCallback").MakePointerType());
CheckImport("disunity_callback_wait", u64, u64, u32, u64, u64);
CheckImport("disunity_coroutine_start", u64, u64, u32, u64, u64, u64.MakePointerType());
CheckImport("disunity_coroutine_stop", u64, u64);
CheckImport("disunity_coroutine_cancelled", u64, u64.MakePointerType());
CheckImport("disunity_input_push", u64, u64, u64, u32, u32, f32);
CheckImport("disunity_input_read", u64, u32, u32, u32.MakePointerType(), f32.MakePointerType());
CheckImport("disunity_physics_create", u64, u64, u32, u64.MakePointerType());
CheckImport("disunity_body_write", u64, u64, AbiType("DisunityBody2d").MakePointerType());
CheckImport("disunity_body_read", u64, u64, AbiType("DisunityBody2d").MakePointerType());
CheckImport("disunity_collider_write", u64, u64, AbiType("DisunityCollider2d").MakePointerType());
CheckImport("disunity_collider_read", u64, u64, AbiType("DisunityCollider2d").MakePointerType());
CheckImport("disunity_physics_raycast", u64, f32, f32, f32, f32, f32, u32, AbiType("DisunityRayHit").MakePointerType());
CheckImport("disunity_physics_overlap_circle", u64, f32, f32, f32, u32, u64.MakePointerType());
CheckImport("disunity_physics_material_create", u64, u64.MakePointerType());
CheckImport("disunity_physics_material_read", u64, u64, AbiType("DisunityPhysicsMaterial2d").MakePointerType());
CheckImport("disunity_physics_material_write", u64, u64, AbiType("DisunityPhysicsMaterial2d").MakePointerType());
CheckImport("disunity_shared_material_read", u64, u64, u64.MakePointerType());
CheckImport("disunity_shared_material_write", u64, u64, u64);
CheckImport("disunity_font_register", u64, bytes, u32, u64.MakePointerType());
CheckImport("disunity_ui_create", u64, u64, u32, u64, u64.MakePointerType());
CheckImport("disunity_rect_write", u64, u64, AbiType("DisunityRect").MakePointerType());
CheckImport("disunity_rect_read", u64, u64, AbiType("DisunityRect").MakePointerType());
CheckImport("disunity_canvas_write", u64, u64, AbiType("DisunityCanvas").MakePointerType());
CheckImport("disunity_canvas_scale_write", u64, u64, AbiType("DisunityCanvasScale").MakePointerType());
CheckImport("disunity_canvas_scale_read", u64, u64, AbiType("DisunityCanvasScale").MakePointerType());
CheckImport("disunity_canvas_scale_factor", u64, u64, u32, u32, f32.MakePointerType());
CheckImport("disunity_canvas_pixel_perfect_write", u64, u64, u32);
CheckImport("disunity_canvas_pixel_perfect_read", u64, u64, u32.MakePointerType());
CheckImport("disunity_text_style_write", u64, u64, AbiType("DisunityTextStyle").MakePointerType());
CheckImport("disunity_text_style_read", u64, u64, AbiType("DisunityTextStyle").MakePointerType());
CheckImport("disunity_tilemap_create", u64, u64, u64.MakePointerType());
CheckImport("disunity_tilemap_write", u64, u64, AbiType("DisunityTilemap").MakePointerType());
CheckImport("disunity_tilemap_read", u64, u64, AbiType("DisunityTilemap").MakePointerType());
CheckImport("disunity_tilemap_set_tile", u64, u64, u32, AbiType("DisunityTile").MakePointerType(), AbiType("DisunitySprite").MakePointerType());
CheckImport("disunity_tilemap_set_tile_shape", u64, u64, u32, f32.MakePointerType(), u32.MakePointerType(), u32);
CheckImport("disunity_tilemap_set_cells", u64, u64, AbiType("DisunityTileCell").MakePointerType(), u32);
CheckImport("disunity_canvas_read", u64, u64, AbiType("DisunityCanvas").MakePointerType());
CheckImport("disunity_text_write", u64, u64, f32, bytes, u32);
CheckImport("disunity_text_read", u64, u64, bytes, u32, u32.MakePointerType(), f32.MakePointerType());
CheckImport("disunity_ui_rect", u64, u64, u32, u32, AbiType("DisunityRect").MakePointerType());
CheckImport("disunity_script_enabled", u64, u64, u32.MakePointerType());
CheckImport("disunity_script_set_enabled", u64, u64, u32, advance.MakePointerType());
CheckImport("disunity_coroutine_ready", u64, u64);
CheckImport("disunity_transform_read", u64, u64, AbiType("DisunityTransform").MakePointerType());
CheckImport("disunity_transform_write", u64, u64, AbiType("DisunityTransform").MakePointerType());
CheckImport("disunity_object_text_write", u64, u64, u32, bytes, u32);
CheckImport("disunity_object_text_read", u64, u64, u32, bytes, u32, u32.MakePointerType());
CheckImport("disunity_object_flags", u64, u64, u32.MakePointerType());
CheckImport("disunity_object_layer_write", u64, u64, u32);
CheckImport("disunity_object_layer_read", u64, u64, u32.MakePointerType());
CheckImport("disunity_world_fixed_delta", u64, typeof(double).MakePointerType());
CheckImport("disunity_body_move_position", u64, u64, f32, f32);
CheckImport("disunity_rotation_read", u64, u64, f32.MakePointerType(), f32.MakePointerType(), f32.MakePointerType(), f32.MakePointerType());
CheckImport("disunity_rotation_write", u64, u64, f32, f32, f32, f32);
CheckImport("disunity_asset_create", u64, u64);
CheckImport("disunity_asset_destroy", u64, u64);
CheckImport("disunity_world_delayed_tasks", u64, u64, u32);
CheckImport("disunity_asset_alive", u64, u64, u32.MakePointerType());
CheckImport("disunity_asset_activate", u64, u64, advance.MakePointerType());
CheckImport("disunity_body_set_sleeping", u64, u64, u32);
CheckImport("disunity_body_sleeping", u64, u64, u32.MakePointerType());
CheckImport("disunity_body_interpolation_write", u64, u64, u32);
CheckImport("disunity_body_interpolation_read", u64, u64, u32.MakePointerType());
CheckImport("disunity_object_alive", u64, u64, u32.MakePointerType());
CheckImport("disunity_callback_check", u64, u64);
CheckImport("disunity_script_retained", u64, u64, u32.MakePointerType());
CheckImport("disunity_sprite_create", u64, u64, u64.MakePointerType());
CheckImport("disunity_sprite_read", u64, u64, AbiType("DisunitySprite").MakePointerType());
CheckImport("disunity_sprite_write", u64, u64, AbiType("DisunitySprite").MakePointerType());
CheckImport("disunity_sprite_bounds", u64, u64, AbiType("DisunityBounds").MakePointerType());
CheckImport("disunity_sprite_validate", AbiType("DisunitySprite").MakePointerType());
CheckImport("disunity_world_export_sprites", u64, AbiType("DisunitySprite").MakePointerType(), AbiType("DisunityMatrix").MakePointerType(), u32, u32.MakePointerType());
CheckImport("disunity_sprite_register", AbiType("DisunitySprite").MakePointerType());
CheckImport("disunity_camera_create", u64, u64, u64.MakePointerType());
CheckImport("disunity_camera_read", u64, u64, AbiType("DisunityCamera").MakePointerType());
CheckImport("disunity_camera_write", u64, u64, AbiType("DisunityCamera").MakePointerType());
CheckImport("disunity_presentation_prepare", u64, u32, u32, u64, AbiType("DisunityPresentation").MakePointerType());
CheckImport("disunity_presentation_quad", u64, u32, AbiType("DisunityQuad").MakePointerType());
CheckImport("disunity_presentation_coverage", u64, u32, AbiType("DisunityCoverage").MakePointerType(), bytes, u32);
CheckImport("disunity_camera_layer_create", u64, u64, u32, u64.MakePointerType());
CheckImport("disunity_camera_layer_read", u64, u64, u32.MakePointerType());
CheckImport("disunity_camera_layer_write", u64, u64, u32);
CheckImport("disunity_audio_listener_count", u64, u32.MakePointerType());
CheckImport("disunity_light2d_read", u64, u64, u32, f32.MakePointerType());
CheckImport("disunity_light2d_write", u64, u64, u32, f32);
CheckImport("disunity_camera_data_read", u64, u64, u32, u32.MakePointerType());
CheckImport("disunity_camera_data_write", u64, u64, u32, u32);
CheckImport("disunity_light2d_layers", u64, u64, typeof(int).MakePointerType(), u32);
CheckImport("disunity_light2d_layers_read", u64, u64, typeof(int).MakePointerType(), u32, u32.MakePointerType());
CheckImport("disunity_light2d_color", u64, u64, f32, f32, f32, f32);
CheckImport("disunity_input_axis_register", u64, u32, bytes, u32, AbiType("DisunityInputAxis").MakePointerType());
CheckImport("disunity_input_axis_read", u64, u32, bytes, u32, u32, f32.MakePointerType());
CheckImport("disunity_input_axis_id", u64, bytes, u32, u32.MakePointerType());
var unverified = bindings.GetMethods(Imports).Select(method => method.Name).Except(checkedImports).ToArray();
Require(unverified.Length == 0, $"native imports without a signature check: {string.Join(", ", unverified)}");
AbiType("NativeLayouts").GetMethod("Verify", BindingFlags.Static | BindingFlags.NonPublic)!.Invoke(null, null);

CheckLayout(result, 16, ("code", typeof(int), 0), ("detail", u64, 8));
CheckLayout(command, 24, ("object", u64, 0), ("kind", u32, 8),
    ("x", f32, 12), ("y", f32, 16), ("z", f32, 20));
CheckLayout(state, 32, ("object", u64, 0), ("x", f32, 8), ("y", f32, 12),
    ("z", f32, 16), ("rotation_y_radians", f32, 20), ("material_slot", u32, 24));
CheckLayout(advance, 32, ("kind", u32, 0), ("message", u32, 4), ("ticket", u64, 8), ("script", u64, 16), ("frame", u64, 24));
CheckLayout(clock, 24, ("delta", typeof(double), 0), ("time", typeof(double), 8), ("frame", u64, 16));
CheckLayout(buffer, 16, ("handle", u64, 0), ("length", u32, 8));
Console.WriteLine($"PASS: all {checkedImports.Count} native import signatures and generated ABI layout expectations");
if (args.Length != 1) { throw new ArgumentException("explicit native library path required"); }
DisunityHost.ConfigureNativeLibrary(args[0]);
ParticipantRegressionTests.Run();
LifecycleTests.Run(args[0]);

ReviewRegressionTests.Run();
HostTests.Run();
CloneTests.Run();
ConstructionTests.Run();
TemplateTests.Run();
PresentationTests.Run();

CoroutineInputTests.Run();

PhysicsTests.Run();
TilemapTests.Run();
PhysicsMaterialTests.Run();

UiTests.Run();
SpriteTests.Run();

EnabledTests.Run();

GameApiTests.Run();
AssetSleepTests.Run();

CameraLayerTests.Run();
PendingCameraComponentTests.Run();
Light2dTests.Run();
CameraDataTests.Run();
PendingCloneTests.Run();

NamedAxisTests.Run();
DeferredDestroyTests.Run();
PlayerAssemblyTests.Run();
RuntimeHookTests.Run();
ScriptLayoutTests.Run();
SynchronizationContextTests.Run();
