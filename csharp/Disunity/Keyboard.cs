namespace UnityEngine.InputSystem;

public sealed class KeyControl
{
    private readonly KeyCode _key;
    internal KeyControl(KeyCode key) => _key = key;
    public bool isPressed => Input.GetKey(_key);
}

public sealed class Keyboard
{
    public static Keyboard current { get; } = new();
    public KeyControl aKey { get; } = new(KeyCode.A);
    public KeyControl dKey { get; } = new(KeyCode.D);
}
