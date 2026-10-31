namespace UnityEngine;

/// <summary>
/// An immediate-mode GUI event: a record of the input a handler is told about. The engine draws no OnGUI layer, so
/// <see cref="current"/> is null, <see cref="PopEvent"/> finds no queued event and <see cref="GetEventCount"/> is 0;
/// scripts that keep an Event of their own can fill it in and read it back. Using an event makes its
/// <see cref="type"/> read <see cref="EventType.Used"/> while <see cref="rawType"/> keeps what it was.
/// </summary>
public sealed class Event
{
    private EventType _type;
    private bool _used;

    public Event() { }

    public Event(int displayIndex) => this.displayIndex = displayIndex;

    public Event(Event other)
    {
        ArgumentNullException.ThrowIfNull(other);
        (_type, _used, mousePosition, delta, button, clickCount, modifiers, pressure, keyCode, character, commandName, displayIndex) =
            (other._type, other._used, other.mousePosition, other.delta, other.button, other.clickCount, other.modifiers, other.pressure,
                other.keyCode, other.character, other.commandName, other.displayIndex);
    }

    public Vector2 mousePosition { get; set; }
    public Vector2 delta { get; set; }
    public int button { get; set; }
    public int clickCount { get; set; }
    public EventModifiers modifiers { get; set; }
    public float pressure { get; set; }
    public KeyCode keyCode { get; set; }
    public char character { get; set; }
    public string commandName { get; set; } = "";
    public int displayIndex { get; set; }

    /// <summary>What the event is, or <see cref="EventType.Used"/> once <see cref="Use"/> was called. Setting it
    /// makes the event unused again.</summary>
    public EventType type
    {
        get => _used ? EventType.Used : _type;
        set => (_type, _used) = (value, false);
    }

    public EventType rawType => _type;

    public bool shift { get => Has(EventModifiers.Shift); set => Set(EventModifiers.Shift, value); }
    public bool control { get => Has(EventModifiers.Control); set => Set(EventModifiers.Control, value); }
    public bool alt { get => Has(EventModifiers.Alt); set => Set(EventModifiers.Alt, value); }
    public bool command { get => Has(EventModifiers.Command); set => Set(EventModifiers.Command, value); }
    public bool capsLock { get => Has(EventModifiers.CapsLock); set => Set(EventModifiers.CapsLock, value); }
    public bool numeric { get => Has(EventModifiers.Numeric); set => Set(EventModifiers.Numeric, value); }
    public bool functionKey => Has(EventModifiers.FunctionKey);

    public bool isKey => type is EventType.KeyDown or EventType.KeyUp;
    public bool isMouse => type is EventType.MouseDown or EventType.MouseUp or EventType.MouseMove or EventType.MouseDrag;
    public bool isScrollWheel => type == EventType.ScrollWheel;

    /// <summary>The event being handled by OnGUI, which the engine never calls, so always null.</summary>
    public static Event? current => null;

    public static int GetEventCount() => 0;

    /// <summary>Takes the next queued event into <paramref name="outEvent"/>; none is ever queued, so false and
    /// <paramref name="outEvent"/> is left alone.</summary>
    public static bool PopEvent(Event outEvent)
    {
        ArgumentNullException.ThrowIfNull(outEvent);
        return false;
    }

    public static Event KeyboardEvent(string key) =>
        throw new NotSupportedException("Event.KeyboardEvent is not supported yet: the engine has no key name parser");

    public void Use() => _used = true;

    public EventType GetTypeForControl(int controlID) => type;

    public override bool Equals(object? obj) =>
        obj is Event other && ReferenceEquals(GetType(), other.GetType()) && type == other.type && modifiers == other.modifiers
        && (isKey ? keyCode == other.keyCode && character == other.character : !isMouse || (button == other.button && mousePosition == other.mousePosition));

    public override int GetHashCode() => HashCode.Combine(type, modifiers, keyCode, character, button);

    public override string ToString() => isKey ? $"Event:{type} char:{(int)character} key:{keyCode} modifiers:{modifiers}" : $"Event:{type}";

    private bool Has(EventModifiers flag) => (modifiers & flag) != 0;

    private void Set(EventModifiers flag, bool on) => modifiers = on ? modifiers | flag : modifiers & ~flag;
}
