using UnityEngine;

/// <summary>The immediate-mode GUI event record the engine never raises.</summary>
internal static class EventTests
{
    public static void Run()
    {
        Defaults();
        Console.WriteLine("PASS: a new Event is empty, and Event.current, PopEvent and GetEventCount report no GUI events");
        Modifiers();
        Console.WriteLine("PASS: Event modifier flags read and write the modifiers value");
        Kinds();
        Console.WriteLine("PASS: Event.isKey, isMouse and isScrollWheel follow the type");
        Used();
        Console.WriteLine("PASS: Event.Use makes type read Used while rawType keeps the type, and setting type clears it");
        Copy();
        Console.WriteLine("PASS: Event copies every field and compares by type, modifiers and key or mouse state");
        Unsupported();
        Console.WriteLine("PASS: Event.KeyboardEvent throws NotSupportedException naming the member");
    }

    private static void Defaults()
    {
        var e = new Event();
        Expect.That(e.type == EventType.MouseDown && e.keyCode == KeyCode.None && e.character == '\0' && e.modifiers == EventModifiers.None && e.commandName == "", "defaults");
        Expect.That(Event.current is null && Event.GetEventCount() == 0, "no current event");
        var untouched = new Event { character = 'q' };
        Expect.That(!Event.PopEvent(untouched) && untouched.character == 'q', "PopEvent leaves its argument alone");
        Expect.Throws<ArgumentNullException>(() => Event.PopEvent(null!), "PopEvent null");
    }

    private static void Modifiers()
    {
        var e = new Event { shift = true, control = true };
        Expect.That(e.modifiers == (EventModifiers.Shift | EventModifiers.Control) && e.shift && e.control && !e.alt && !e.command, "shift and control");
        e.shift = false;
        e.command = true;
        e.capsLock = true;
        e.numeric = true;
        Expect.That(e.modifiers == (EventModifiers.Control | EventModifiers.Command | EventModifiers.CapsLock | EventModifiers.Numeric), "flags change independently");
        Expect.That(!e.functionKey, "function key is not set");
        e.modifiers = EventModifiers.FunctionKey | EventModifiers.Alt;
        Expect.That(e.functionKey && e.alt && !e.control, "the modifiers value sets the flags");
    }

    private static void Kinds()
    {
        Expect.That(new Event { type = EventType.KeyDown }.isKey && new Event { type = EventType.KeyUp }.isKey, "key events");
        Expect.That(new Event { type = EventType.MouseDrag }.isMouse && !new Event { type = EventType.KeyDown }.isMouse, "mouse events");
        Expect.That(new Event { type = EventType.ScrollWheel }.isScrollWheel && !new Event { type = EventType.Layout }.isScrollWheel, "scroll wheel");
        Expect.That(new Event { type = EventType.Repaint }.GetTypeForControl(3) == EventType.Repaint, "type for control");
    }

    private static void Used()
    {
        var e = new Event { type = EventType.KeyDown };
        e.Use();
        Expect.That(e.type == EventType.Used && e.rawType == EventType.KeyDown && !e.isKey, "a used event reads Used");
        e.type = EventType.KeyUp;
        Expect.That(e.type == EventType.KeyUp && e.rawType == EventType.KeyUp, "setting the type clears use");
    }

    private static void Copy()
    {
        var a = new Event { type = EventType.KeyDown, keyCode = KeyCode.A, character = 'a', modifiers = EventModifiers.Shift, mousePosition = new Vector2(1, 2),
            delta = new Vector2(3, 4), button = 1, clickCount = 2, pressure = 0.5f, commandName = "Copy", displayIndex = 3 };
        var b = new Event(a);
        Expect.That(b.type == EventType.KeyDown && b.keyCode == KeyCode.A && b.character == 'a' && b.shift && b.mousePosition == new Vector2(1, 2)
            && b.delta == new Vector2(3, 4) && b.button == 1 && b.clickCount == 2 && b.pressure == 0.5f && b.commandName == "Copy" && b.displayIndex == 3, "copy fields");
        Expect.That(a.Equals(b) && a.GetHashCode() == b.GetHashCode(), "a copy is equal");
        b.character = 'b';
        Expect.That(!a.Equals(b), "a different character is a different key event");
        Expect.That(new Event(7).displayIndex == 7, "display index constructor");
        Expect.Throws<ArgumentNullException>(() => new Event((Event)null!), "copy of null");
    }

    private static void Unsupported() =>
        Expect.That(Expect.Catch<NotSupportedException>(() => Event.KeyboardEvent("a"), "KeyboardEvent").Message.Contains("Event.KeyboardEvent"), "message names the member");
}
