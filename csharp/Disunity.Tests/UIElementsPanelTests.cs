using Disunity.Hosting;
using UnityEngine.UIElements;

/// <summary>The UI Toolkit panel registry: a player has no panels, and a new panel is not disposed.</summary>
internal static class UIElementsPanelTests
{
    public static void Run()
    {
        NoPanelsExist();
        SubscriptionsAreAccepted();
        APanelStartsLive();
        DisunityHost.LoadWorld();
        try { EventSystemsRegister(); DestroyHelper(); }
        finally { DisunityHost.UnloadWorld(); }
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

    private static void APanelStartsLive()
    {
        var panel = new TestPanel();
        Expect.That(!panel.disposed, "a new panel is not disposed");
        Expect.That(panel.isFlat, "a new panel is flat");
    }

    private static void EventSystemsRegister()
    {
        var eventSystem = new UnityEngine.GameObject("EventSystem");
        UIElementsRuntimeUtility.RegisterEventSystem(eventSystem);
        UIElementsRuntimeUtility.UnregisterEventSystem(eventSystem);
        Expect.Throws<ArgumentNullException>(() => UIElementsRuntimeUtility.RegisterEventSystem(null!), "register null");
        Expect.Throws<ArgumentNullException>(() => UIElementsRuntimeUtility.UnregisterEventSystem(null!), "unregister null");
    }

    private static void DestroyHelper()
    {
        UIRUtility.Destroy(null);
        var panel = new UnityEngine.GameObject("Panel");
        UIRUtility.Destroy(panel);
        Expect.That(panel, "the object lives until the end of the frame");
        DisunityHost.Step(0);
        Expect.That(!panel, "the object is destroyed");
    }

    private sealed class TestPanel : BaseRuntimePanel
    {
    }
}
