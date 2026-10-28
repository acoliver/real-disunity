using Unity.Profiling;
using Unity.Profiling.LowLevel;
using Unity.Profiling.LowLevel.Unsafe;
using Unity.Profiling.Memory;
using UnityEngine;

/// <summary>Profiler markers, categories, recorders and the memory snapshot metadata.</summary>
internal static class ProfilingTests
{
    public static void Run()
    {
        Markers();
        Categories();
        Recorders();
        Console.WriteLine("PASS: ProfilerMarker, ProfilerCategory, ProfilerRecorder and MemorySnapshotMetadata");
    }

    private static void Markers()
    {
        var first = new ProfilerMarker("Test.Marker");
        var again = new ProfilerMarker(ProfilerCategory.Scripts, "Test.Marker", MarkerFlags.Script);
        var other = new ProfilerMarker("Test.Other");
        Expect.That(first.Handle != IntPtr.Zero, "a marker's handle is not zero");
        Expect.That(first.Handle == again.Handle, "markers of one name share a handle");
        Expect.That(first.Handle != other.Handle, "markers of different names have different handles");
        Expect.That(default(ProfilerMarker).Handle == IntPtr.Zero, "a default marker has no handle");

        first.Begin();
        first.Begin(new Texture2D(1, 1));
        first.End();
        using (first.Auto()) { }
        var scope = first.Auto();
        scope.Dispose();
        scope.Dispose();
        Expect.Throws<ArgumentNullException>(() => _ = new ProfilerMarker((string)null!), "a marker needs a name");
        unsafe
        {
            fixed (char* name = "Test.Marker") Expect.That(new ProfilerMarker(name, 11).Handle == first.Handle, "a marker built from characters finds the same handle");
        }
    }

    private static void Categories()
    {
        Expect.Equal("Render", ProfilerCategory.Render.Name, "Render name");
        Expect.Equal("GUI", ProfilerCategory.Gui.Name, "Gui name");
        Expect.Equal("AI", ProfilerCategory.Ai.Name, "Ai name");
        Expect.Equal("VR", ProfilerCategory.Vr.Name, "Vr name");
        Expect.Equal("Virtual Texturing", ProfilerCategory.VirtualTexturing.Name, "VirtualTexturing name");
        Expect.Equal("File IO", ProfilerCategory.FileIO.Name, "FileIO name");
        Expect.Equal("Physics2D", ProfilerCategory.Physics2D.ToString(), "ToString is the name");
        var custom = new ProfilerCategory("Mine", ProfilerCategoryColor.Audio);
        Expect.Equal("Mine", custom.Name, "custom name");
        Expect.That(custom.Color.Equals(ProfilerCategory.Audio.Color), "a custom category takes its color's palette entry");
        Expect.That(!ProfilerCategory.Render.Color.Equals(ProfilerCategory.Memory.Color), "categories of different colors differ");
        Expect.That((ushort)custom == (ushort)new ProfilerCategory("Mine"), "a name always has the same id");
        Expect.That((ushort)custom != (ushort)ProfilerCategory.Render, "different names have different ids");
        Expect.Equal("", default(ProfilerCategory).Name, "a default category has an empty name");
    }

    private static void Recorders()
    {
        var marker = new ProfilerMarker("Test.Recorded");
        var recorder = ProfilerRecorder.StartNew(marker);
        AssertEmpty(recorder);
        AssertEmpty(ProfilerRecorder.StartNew(ProfilerCategory.Memory, "System Used Memory", 15, ProfilerRecorderOptions.Default));
        AssertEmpty(new ProfilerRecorder("Total Reserved Memory", 1, ProfilerRecorderOptions.StartImmediately));
        AssertEmpty(new ProfilerRecorder(default(ProfilerRecorderHandle)));
        recorder.Start();
        Expect.That(!recorder.IsRunning, "a recorder with nothing to record does not run");
        recorder.Dispose();
        recorder.Dispose();

        var handles = new List<ProfilerRecorderHandle> { default };
        ProfilerRecorderHandle.GetAvailable(handles);
        Expect.Equal(0, handles.Count, "the engine publishes no statistics");
        Expect.Throws<ArgumentException>(() => ProfilerRecorderHandle.GetDescription(default), "a handle names nothing");

        var metadata = new MemorySnapshotMetadata();
        Expect.Equal("", metadata.Description, "description starts empty");
        metadata.Description = "level 1";
        Expect.Equal("level 1", metadata.Description, "description is stored");
    }

    private static void AssertEmpty(ProfilerRecorder recorder)
    {
        Expect.That(!recorder.Valid && !recorder.IsRunning && !recorder.WrappedAround, "an engine recorder is not valid");
        Expect.That(recorder.CurrentValue == 0 && recorder.LastValue == 0 && recorder.CurrentValueAsDouble == 0 && recorder.LastValueAsDouble == 0, "values read 0");
        Expect.That(recorder.Count == 0 && recorder.Capacity == 0 && recorder.ToArray().Length == 0, "no samples");
        Expect.Throws<ArgumentOutOfRangeException>(() => recorder.GetSample(0), "no sample can be read");
        var samples = new List<ProfilerRecorderSample>();
        recorder.CopyTo(samples);
        Expect.Equal(0, samples.Count, "nothing is copied");
        recorder.Stop();
        recorder.Reset();
    }
}
