namespace UnityEngine;

/// <summary>The exception the engine throws for a misuse of engine state that a game can catch, such as reading the pixels
/// of a texture that is not readable.</summary>
public class UnityException : Exception
{
    public UnityException()
    {
    }

    public UnityException(string message) : base(message)
    {
    }

    public UnityException(string message, Exception innerException) : base(message, innerException)
    {
    }
}
