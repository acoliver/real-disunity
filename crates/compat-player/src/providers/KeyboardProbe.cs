using System.Security.Cryptography;
using System.Text.Json;
using UnityEngine;
using UnityEngine.InputSystem;

public sealed class ProviderKeyboardProbe : MonoBehaviour
{
    private string dynamicSample = "";
    private string fixedSample = "";
    public string DynamicSample => dynamicSample;
    public string FixedSample => fixedSample;
    public string LoadedImages => JsonSerializer.Serialize(new[] {
        typeof(ProviderKeyboardProbe).Assembly.Location,
        typeof(Keyboard).Assembly.Location,
        typeof(CompatibilityBridge.ManagedBridge).Assembly.Location
    }.Select(path => new { path, sha256 = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path))).ToLowerInvariant() }));
    private void Update() => dynamicSample = Sample();
    private void FixedUpdate() => fixedSample = Sample();
    private static string Sample()
    {
        var keyboard = Keyboard.current;
        return JsonSerializer.Serialize(new {
            held = new[] { keyboard.wKey.isPressed, keyboard.aKey.isPressed, keyboard.sKey.isPressed, keyboard.dKey.isPressed },
            down = new[] { keyboard.wKey.wasPressedThisFrame, keyboard.aKey.wasPressedThisFrame, keyboard.sKey.wasPressedThisFrame, keyboard.dKey.wasPressedThisFrame },
            up = new[] { keyboard.wKey.wasReleasedThisFrame, keyboard.aKey.wasReleasedThisFrame, keyboard.sKey.wasReleasedThisFrame, keyboard.dKey.wasReleasedThisFrame },
            repeated = keyboard.wKey.wasPressedThisFrame == keyboard.wKey.wasPressedThisFrame,
            vertical = Input.GetAxisRaw("Vertical"), alternate = Input.GetAxisRaw("AlternateVertical")
        });
    }
}

public sealed class ProviderSpriteProbe : MonoBehaviour
{
    private int step;
    private void Update()
    {
        step++;
        var sprite = GetComponent<SpriteRenderer>() ?? throw new InvalidOperationException("qualification sprite is absent");
        if (step == 1)
        {
            transform.position = new Vector3(1, 0, 0);
            transform.localScale = new Vector3(0.5f, 1, 1);
            var tile = GameObject.Find("canonical-tiles") ?? throw new InvalidOperationException("qualification tile is absent");
            tile.transform.position = new Vector3(-1.5f, 0, 0);
        }
        if (step == 2) sprite.color = new Color(1, 0.25f, 1, 0.5f);
        if (step == 3) sprite.enabled = false;
        if (step == 4) { sprite.enabled = true; sprite.color = new Color(1, 1, 1, 1); NativeWorld.Request("component.set", ("component", sprite.NativeHandle), ("fields", new { flip_y = true })).Dispose(); }
        if (step == 5) gameObject.SetActive(false);
    }
}
