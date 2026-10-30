using UnityEngine.UIElements;

/// <summary>The UI Toolkit panel registry: a player has no panels, and a new panel is not disposed.</summary>
internal static class UIElementsPanelTests
{
    public static void Run()
    {
        NoPanelsExist();
        SubscriptionsAreAccepted();
        APanelStartsLive();
        Console.WriteLine("PASS: UIElementsRuntimeUtility reports no panels, accepts subscriptions and a panel starts live");
    }

    private static void NoPanelsExist()
    {
        var panels = UIElementsRuntimeUtility.GetSortedPlayerPanels();
        Expect.Equal(0, panels.Count, "no player panels");
        panels.Add(new TestPanel());
        Expect.Equal(0, UIElementsRuntimeUtility.GetSortedPlayerPanels().Count, "a caller's list does not become the registry");
    }

    private static void SubscriptionsAreAccepted()
    {
        var created = 0;
        Action<BaseRuntimePanel> handler = _ => created++;
        UIElementsRuntimeUtility.onCreatePanel += handler;
        UIElementsRuntimeUtility.onCreatePanel -= handler;
        Expect.Equal(0, created, "no panel is created");
    }

    private static void APanelStartsLive() => Expect.That(!new TestPanel().disposed, "a new panel is not disposed");

    private sealed class TestPanel : BaseRuntimePanel
    {
    }
}
