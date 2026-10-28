using Disunity.Hosting;

/// <summary>A native error as game code sees it. The engine fails the step for these, where it
/// logs and continues past an exception the game itself throws.</summary>
internal static class EngineFault
{
    public static DisunityException Make(string message) => new(1, DisunityErrorReason.CallbackFailed, message);
}
