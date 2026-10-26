using Disunity.Native;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private static string? _listenerDiagnostic;
    private static void ReportAudioListenerDiagnostic()
    {
        var diagnostic = AudioListenerDiagnostic;
        if (diagnostic != _listenerDiagnostic && diagnostic is not null) Console.Error.WriteLine(diagnostic);
        _listenerDiagnostic = diagnostic;
    }

    public static unsafe string? AudioListenerDiagnostic
    {
        get
        {
            uint count;
            ThrowIfError(NativeBindings.disunity_audio_listener_count(WorldHandle, &count));
            return count switch
            {
                1 => null,
                0 => "There are no audio listeners in the scene. Please ensure there is always one audio listener in the scene.",
                _ => $"There are {count} audio listeners in the scene. Please ensure there is always exactly one audio listener in the scene."
            };
        }
    }
}
