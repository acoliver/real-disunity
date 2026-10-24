using UnityEngine;
using UnityEngine.UI;

internal static class UiFacadeBehavior
{
    public static void Run()
    {
        var root = new GameObject("ui-root");
        var rect = root.AddComponent<RectTransform>();
        rect.sizeDelta = new Vector2(400, 200);
        var child = new GameObject("ui-child");
        var childRect = child.AddComponent<RectTransform>();
        childRect.SetParent(root.transform, false);
        childRect.anchorMin = Vector2.zero;
        childRect.anchorMax = Vector2.one;
        childRect.offsetMin = new Vector2(10, 20);
        childRect.offsetMax = new Vector2(-30, -40);
        var bounds = childRect.rect;
        BinderBehavior.Check(bounds.x == -180 && bounds.y == -70 && bounds.width == 360 && bounds.height == 140, "RectTransform.rect exposes native anchor layout with no managed layout model");
        BinderBehavior.Check(ReferenceEquals(child.transform, childRect), "RectTransform inherits Transform and retains native component identity");
        var text = child.AddComponent<Text>();
        text.text = "17";
        using (var snapshot = NativeWorld.Request("component.get", ("component", text.NativeHandle)))
            BinderBehavior.Check(NativeWorld.Data(snapshot).GetProperty("fields").GetProperty("text").GetString() == "17", "Text setter changes actual native Text");
        NativeWorld.Request("component.set", ("component", text.NativeHandle), ("fields", new { text = "23" })).Dispose();
        BinderBehavior.Check(text.text == "23", "Text getter observes native updates without stale managed cache");
        Console.WriteLine("PASS native UI facade: native RectTransform rect/anchor layout, derived identity and bidirectional native Text fields");
    }
}
