namespace UnityEngine;

/// <summary>Thrown when a script uses a component its object does not have.</summary>
public class MissingComponentException : SystemException
{
    public MissingComponentException()
    {
    }

    public MissingComponentException(string message)
        : base(message)
    {
    }

    public MissingComponentException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}
