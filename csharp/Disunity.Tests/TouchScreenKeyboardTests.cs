using UnityEngine;

/// <summary>The on-screen keyboard the engine does not have.</summary>
internal static class TouchScreenKeyboardTests
{
    public static void Run()
    {
        Statics();
        Console.WriteLine("PASS: TouchScreenKeyboard reports no support, no in-place editing, no visible keyboard and an empty area");
        HideInput();
        Console.WriteLine("PASS: TouchScreenKeyboard.hideInput stores the value set");
        StatusValues();
        Console.WriteLine("PASS: TouchScreenKeyboard.Status has Visible, Done, Canceled and LostFocus with their values");
        OpenThrows();
        Console.WriteLine("PASS: TouchScreenKeyboard.Open throws NotSupportedException naming the member for every overload");
        NoInstance();
        Console.WriteLine("PASS: TouchScreenKeyboard has no public constructor and RangeInt reports its end");
    }

    private static void Statics()
    {
        Expect.That(!TouchScreenKeyboard.isSupported, "isSupported is false");
        Expect.That(!TouchScreenKeyboard.isInPlaceEditingAllowed, "in-place editing is not allowed");
        Expect.That(!TouchScreenKeyboard.visible, "no keyboard is visible");
        Expect.That(TouchScreenKeyboard.area == Rect.zero, "the keyboard area is empty");
    }

    private static void HideInput()
    {
        var before = TouchScreenKeyboard.hideInput;
        TouchScreenKeyboard.hideInput = !before;
        Expect.That(TouchScreenKeyboard.hideInput == !before, "hideInput reads back what was set");
        TouchScreenKeyboard.hideInput = before;
        Expect.That(TouchScreenKeyboard.hideInput == before, "hideInput can be set back");
    }

    private static void StatusValues()
    {
        Expect.That((int)TouchScreenKeyboard.Status.Visible == 0 && (int)TouchScreenKeyboard.Status.Done == 1
            && (int)TouchScreenKeyboard.Status.Canceled == 2 && (int)TouchScreenKeyboard.Status.LostFocus == 3, "Status values");
        Expect.That(Enum.GetValues<TouchScreenKeyboard.Status>().Length == 4, "Status has four members");
    }

    private static void OpenThrows()
    {
        var error = Expect.Catch<NotSupportedException>(() => TouchScreenKeyboard.Open("x"), "Open(text)");
        Expect.That(error.Message.Contains("TouchScreenKeyboard.Open"), "the message names the member");
        Expect.Throws<NotSupportedException>(() => TouchScreenKeyboard.Open("x", TouchScreenKeyboardType.URL, true, true, true, true, "p", 5), "Open with every argument");
        Expect.Throws<NotSupportedException>(() => TouchScreenKeyboard.Open("x", TouchScreenKeyboardType.URL, true, true, true, true, "p"), "Open with seven arguments");
    }

    private static void NoInstance()
    {
        Expect.That(typeof(TouchScreenKeyboard).GetConstructors().Length == 0, "no public constructor");
        var range = new RangeInt(3, 4);
        Expect.That(range.start == 3 && range.length == 4 && range.end == 7, "RangeInt end is start plus length");
    }
}
