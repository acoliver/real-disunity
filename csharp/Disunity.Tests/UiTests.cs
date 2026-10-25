using Disunity.Hosting;
using UnityEngine;
using UnityEngine.UI;

internal static class UiTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        var root = DisunityHost.CreateGameObject();
        var rect = root.AddComponent<RectTransform>();
        root.AddComponent<Canvas>();
        Require(ReferenceEquals(root.transform, rect), "RectTransform replaces transform wrapper");
        Require(rect.rect.width == 800 && rect.rect.height == 600, "native screen Canvas layout");
        var label = DisunityHost.CreateGameObject();
        var child = label.AddComponent<RectTransform>();
        child.SetParent(rect, false);
        child.sizeDelta = new Vector2(200, 50);
        var font = DisunityHost.RegisterFont(SyntheticFont.Bytes());
        var text = DisunityHost.CreateText(label, font);
        text.text = "Score: 1";
        Require(text.text == "Score: 1", "native text write/read");
        text.text = "Score: 2";
        text.fontSize = 20;
        Require(text.text == "Score: 2" && text.fontSize == 20, "native text updates");
        Require(child.rect.width == 200 && child.rect.height == 50, "native child rectangle");
        try { text.text = "unsupported λ"; throw new Exception("unsupported text accepted"); }
        catch (DisunityException) { }
        Require(text.text == "Score: 2", "invalid text leaves old native text intact");
        var copiedText = UnityEngine.Object.Instantiate(text);
        Require(copiedText.text == "Score: 2", "cloned native text wrapper");
        copiedText.text = "Copy";
        Require(text.text == "Score: 2", "cloned text edits are independent");
        var clone = UnityEngine.Object.Instantiate(root);
        Require(clone.transform is RectTransform, "cloned rectangle keeps one transform wrapper");
        Require(((RectTransform)clone.transform).rect.width == 800, "cloned native Canvas");
        DisunityHost.SetViewport(1200, 900);
        Require(rect.rect.width == 1200 && rect.rect.height == 900, "native Canvas resize");
        DisunityHost.SetViewport(800, 600);
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: native Canvas layout, RectTransform and text updates");
    }
    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
