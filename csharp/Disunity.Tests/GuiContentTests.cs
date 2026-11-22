using UnityEngine;

/// <summary>GUIContent holds a text, an image and a tooltip, turns null text into empty text and shares one scratch value.</summary>
internal static class GuiContentTests
{
    public static void Run()
    {
        var empty = new GUIContent();
        Expect.That(empty.text == "" && empty.tooltip == "" && empty.image == null, "a new content is empty");
        var text = new GUIContent("label");
        Expect.That(text.text == "label" && text.ToString() == "label", "text and ToString");
        var tipped = new GUIContent("label", "hint");
        Expect.That(tipped.text == "label" && tipped.tooltip == "hint", "text and tooltip");
        var all = new GUIContent("a", null, "b");
        Expect.That(all.text == "a" && all.tooltip == "b" && all.image == null, "text, no image and tooltip");
        var nulls = new GUIContent((string?)null);
        Expect.That(nulls.text == "", "a null text is empty");
        nulls.tooltip = null!;
        Expect.That(nulls.tooltip == "", "a null tooltip is empty");
        var copy = new GUIContent(all);
        copy.text = "changed";
        Expect.That(all.text == "a" && copy.tooltip == "b", "a copy is separate");
        Expect.Throws<ArgumentNullException>(() => _ = new GUIContent((GUIContent)null!), "copying null");
        Expect.That(ReferenceEquals(GUIContent.none, GUIContent.none) && GUIContent.none.text == "", "none is one empty value");
        var first = GUIContent.Temp("one", "tip");
        var second = GUIContent.Temp("two");
        Expect.That(ReferenceEquals(first, second) && second.text == "two" && second.tooltip == "", "Temp reuses one value and resets the other parts");
        var held = new Texture2D(2, 2);
        var withImage = GUIContent.Temp("kept", held);
        GUIContent.ClearStaticCache();
        Expect.That(withImage.text == "" && withImage.image == null && withImage.tooltip == "", "ClearStaticCache empties the scratch value, so it holds no text, image or tooltip");
        Expect.That(ReferenceEquals(withImage, GUIContent.Temp("after")) && withImage.text == "after", "the scratch value is still the one Temp returns");
        Console.WriteLine("PASS: GUIContent holds text, image and tooltip, treats null text as empty, shares one scratch value and empties it with ClearStaticCache");
    }
}
