using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Command buffers attached to cameras and lights by event.</summary>
internal static class CommandBufferAttachmentTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            CameraOrder();
            CameraRemoval();
            Arguments();
            Console.WriteLine("PASS: Camera command buffers attach by event in order, and remove by buffer, event or all");
            LightAttachments();
            Clones();
            Console.WriteLine("PASS: Light command buffers attach by event; Instantiate copies no attachments");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static Camera NewCamera() => DisunityHost.CreateGameObject().AddComponent<Camera>();

    private static void CameraOrder()
    {
        var camera = NewCamera();
        var (first, second) = (new CommandBuffer { name = "first" }, new CommandBuffer { name = "second" });
        Ensure(camera.GetCommandBuffers(CameraEvent.AfterEverything).Length == 0 && camera.commandBufferCount == 0, "a camera starts with none");
        camera.AddCommandBuffer(CameraEvent.AfterEverything, first);
        camera.AddCommandBuffer(CameraEvent.AfterEverything, second);
        camera.AddCommandBuffer(CameraEvent.AfterEverything, first);
        camera.AddCommandBuffer(CameraEvent.BeforeImageEffects, second);
        var after = camera.GetCommandBuffers(CameraEvent.AfterEverything);
        Ensure(after.Length == 3 && after[0] == first && after[1] == second && after[2] == first, "in the order added, a buffer may repeat");
        Ensure(camera.GetCommandBuffers(CameraEvent.BeforeImageEffects).Single() == second, "each event keeps its own list");
        Ensure(camera.commandBufferCount == 4, "the count covers every event");
        after[0] = second;
        Ensure(camera.GetCommandBuffers(CameraEvent.AfterEverything)[0] == first, "the array returned is a copy");
        Ensure(NewCamera().commandBufferCount == 0, "another camera has its own lists");
    }

    private static void CameraRemoval()
    {
        var camera = NewCamera();
        var (kept, removed) = (new CommandBuffer(), new CommandBuffer());
        camera.AddCommandBuffer(CameraEvent.AfterEverything, removed);
        camera.AddCommandBuffer(CameraEvent.AfterEverything, kept);
        camera.AddCommandBuffer(CameraEvent.AfterEverything, removed);
        camera.AddCommandBuffer(CameraEvent.BeforeSkybox, removed);
        camera.RemoveCommandBuffer(CameraEvent.AfterEverything, removed);
        Ensure(camera.GetCommandBuffers(CameraEvent.AfterEverything).Single() == kept, "removing a buffer removes every attachment at the event");
        Ensure(camera.GetCommandBuffers(CameraEvent.BeforeSkybox).Single() == removed, "and leaves other events");
        camera.RemoveCommandBuffer(CameraEvent.AfterEverything, new CommandBuffer());
        Ensure(camera.commandBufferCount == 2, "removing a buffer that is not attached changes nothing");
        camera.RemoveCommandBuffers(CameraEvent.AfterEverything);
        Ensure(camera.GetCommandBuffers(CameraEvent.AfterEverything).Length == 0 && camera.commandBufferCount == 1, "RemoveCommandBuffers empties one event");
        camera.RemoveAllCommandBuffers();
        Ensure(camera.commandBufferCount == 0, "RemoveAllCommandBuffers empties every event");
    }

    private static void Arguments()
    {
        var camera = NewCamera();
        Throws<ArgumentNullException>(() => camera.AddCommandBuffer(CameraEvent.AfterEverything, null!), "a null buffer to add");
        Throws<ArgumentNullException>(() => camera.RemoveCommandBuffer(CameraEvent.AfterEverything, null!), "a null buffer to remove");
        Throws<ArgumentOutOfRangeException>(() => camera.AddCommandBuffer((CameraEvent)99, new CommandBuffer()), "an undefined event",
            "Camera.AddCommandBuffer", "99 is not a CameraEvent");
        Throws<ArgumentOutOfRangeException>(() => camera.GetCommandBuffers((CameraEvent)(-1)), "an undefined event to read",
            "Camera.GetCommandBuffers");
        Ensure(camera.commandBufferCount == 0, "a rejected call attaches nothing");
    }

    private static void LightAttachments()
    {
        var light = DisunityHost.CreateGameObject().AddComponent<Light>();
        var (buffer, other) = (new CommandBuffer(), new CommandBuffer());
        light.AddCommandBuffer(LightEvent.AfterShadowMap, buffer);
        light.AddCommandBuffer(LightEvent.AfterShadowMap, other);
        light.AddCommandBuffer(LightEvent.BeforeShadowMap, buffer);
        Ensure(light.GetCommandBuffers(LightEvent.AfterShadowMap).SequenceEqual([buffer, other]) && light.commandBufferCount == 3, "a light attaches by event");
        light.RemoveCommandBuffer(LightEvent.AfterShadowMap, buffer);
        Ensure(light.GetCommandBuffers(LightEvent.AfterShadowMap).Single() == other && light.commandBufferCount == 2, "and removes one buffer from one event");
        light.RemoveCommandBuffers(LightEvent.AfterShadowMap);
        Ensure(light.commandBufferCount == 1, "RemoveCommandBuffers on a light empties one event");
        Throws<ArgumentOutOfRangeException>(() => light.AddCommandBuffer((LightEvent)40, buffer), "an undefined light event", "Light.AddCommandBuffer");
        light.RemoveAllCommandBuffers();
        Ensure(light.commandBufferCount == 0, "RemoveAllCommandBuffers on a light");
    }

    private static void Clones()
    {
        var camera = NewCamera();
        var light = camera.gameObject.AddComponent<Light>();
        camera.AddCommandBuffer(CameraEvent.AfterEverything, new CommandBuffer());
        light.AddCommandBuffer(LightEvent.AfterShadowMap, new CommandBuffer());
        var clone = UnityEngine.Object.Instantiate(camera.gameObject);
        Ensure(clone.GetComponent<Camera>()!.commandBufferCount == 0, "a cloned camera has no command buffers");
        Ensure(clone.GetComponent<Light>()!.commandBufferCount == 0, "a cloned light has no command buffers");
        Ensure(camera.commandBufferCount == 1 && light.commandBufferCount == 1, "the original keeps its own");
    }
}
