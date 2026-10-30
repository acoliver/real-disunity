using Disunity.Hosting;
using UnityEngine;
using UnityEngine.UIElements;

/// <summary>The UI Toolkit input configuration an event system reads: defaults, validation and no current panel.</summary>
internal static class PanelInputConfigurationTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Defaults();
            NoCurrentConfiguration();
            SettingsCanBeReplaced();
            ApplyCallbackStartsEmpty();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: PanelInputConfiguration defaults, no current configuration and validated settings");
    }

    private static void Defaults()
    {
        var settings = PanelInputConfiguration.Settings.Default;
        Expect.Equal(PanelInputConfiguration.PanelInputRedirection.AutoSwitch, settings.panelInputRedirection, "redirection");
        Expect.That(settings.processWorldSpaceInput, "world space input is processed");
        Expect.Equal(-1, settings.interactionLayers.value, "every layer interacts");
        Expect.That(float.IsPositiveInfinity(settings.maxInteractionDistance), "the interaction distance is unlimited");
        Expect.That(settings.autoCreatePanelComponents, "panel components are created automatically");
        Expect.Equal(0, default(PanelInputConfiguration.Settings).interactionLayers.value, "a default struct is zeroed");
    }

    private static void NoCurrentConfiguration()
    {
        Expect.That(PanelInputConfiguration.current == null, "a world has no UI Toolkit panels");
        var configuration = new GameObject("Panels").AddComponent<PanelInputConfiguration>();
        Expect.That(PanelInputConfiguration.current == null, "a component in the scene is not a current configuration");
        Expect.Equal(PanelInputConfiguration.PanelInputRedirection.AutoSwitch, configuration.settings.panelInputRedirection, "a new component has the defaults");
    }

    private static void ApplyCallbackStartsEmpty()
    {
        Expect.That(PanelInputConfiguration.onApply == null, "nothing subscribes to apply by default");
        var calls = 0;
        PanelInputConfiguration.onApply += _ => calls++;
        try
        {
            var configuration = new GameObject("Panels").AddComponent<PanelInputConfiguration>();
            configuration.settings = PanelInputConfiguration.Settings.Default;
            Expect.Equal(0, calls, "the engine applies no panel input, so it raises nothing");
        }
        finally { PanelInputConfiguration.onApply = null; }
    }

    private static void SettingsCanBeReplaced()
    {
        var configuration = new GameObject("Panels").AddComponent<PanelInputConfiguration>();
        var settings = PanelInputConfiguration.Settings.Default;
        settings.panelInputRedirection = PanelInputConfiguration.PanelInputRedirection.Never;
        settings.maxInteractionDistance = 12.5f;
        configuration.settings = settings;
        Expect.Equal(PanelInputConfiguration.PanelInputRedirection.Never, configuration.settings.panelInputRedirection, "redirection is stored");
        Expect.Equal(12.5f, configuration.settings.maxInteractionDistance, "distance is stored");

        settings.panelInputRedirection = (PanelInputConfiguration.PanelInputRedirection)9;
        Expect.Throws<ArgumentOutOfRangeException>(() => configuration.settings = settings, "unknown redirection");
        Expect.Equal(PanelInputConfiguration.PanelInputRedirection.Never, configuration.settings.panelInputRedirection, "a rejected value changes nothing");
    }
}
