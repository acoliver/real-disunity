namespace Disunity.Hosting;

/// <summary>The input axes a player starts with, from the <c>axes</c> section of the resource plan, which lists them
/// as a scene plan does.</summary>
internal sealed partial class ResourceDatabase
{
    /// <summary>Registers the planned axes and logs each one the engine cannot deliver.</summary>
    public void ApplyInputAxes(Action<string> log)
    {
        RequireMainThread();
        if (_axes is not { } axes) return;
        foreach (var line in InputAxes.Register(axes)) log($"input axis not registered: {line}");
    }
}
