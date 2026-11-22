using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

namespace PortableFixture;

/// <summary>A script whose screen and overlay both change in every Update, so a captured frame shows which update drew it. Each
/// Update clears the screen to yellow when its count is even and to cyan when it is odd, and moves the screen UI fixture's green
/// rectangle 8 pixels right for each update after the first two. The first Update runs while the scene loads and the second in the
/// first frame the host advances, so game frame N is the Update counted N + 2: yellow for an even frame, cyan for an odd one, with
/// the rectangle 8 * N pixels from where the screen UI fixture puts it. The colours use only the channel values 0 and 1, which
/// survive any colour-space encoding.</summary>
public sealed class ScreenCountFixture : MonoBehaviour
{
    private const int LoadingUpdates = 2;
    private const float StepPixels = 8f;

    private CommandBuffer _yellow = null!;
    private CommandBuffer _cyan = null!;
    private RectTransform _overlay = null!;
    private int _updates;

    public static void Create()
    {
        var fixture = DisunityHost.CreateGameObject().AddComponent<ScreenCountFixture>();
        fixture._yellow = Clear(new Color(1f, 1f, 0f, 1f));
        fixture._cyan = Clear(new Color(0f, 1f, 1f, 1f));
        fixture._overlay = ScreenUiFixture.AddOverlay();
    }

    private static CommandBuffer Clear(Color color)
    {
        var buffer = new CommandBuffer();
        buffer.SetRenderTarget(BuiltinRenderTextureType.CameraTarget);
        buffer.ClearRenderTarget(true, true, color);
        return buffer;
    }

    private void Update()
    {
        _updates++;
        Graphics.ExecuteCommandBuffer(_updates % 2 == 0 ? _yellow : _cyan);
        _overlay.anchoredPosition = new Vector2(StepPixels * Math.Max(0, _updates - LoadingUpdates), 0);
    }
}
