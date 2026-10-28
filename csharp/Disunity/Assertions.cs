namespace UnityEngine.Assertions;

/// <summary>The exception a failed assertion throws. Its message is the assertion's message,
/// followed on a new line by the caller's message when there is one.</summary>
public class AssertionException : Exception
{
    private readonly string? _userMessage;

    public AssertionException(string message, string? userMessage) : base(message) => _userMessage = userMessage;

    public override string Message => _userMessage is null ? base.Message : base.Message + "\n" + _userMessage;
}
