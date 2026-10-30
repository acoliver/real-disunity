using UnityEngine;

/// <summary>The subsystem registry has no entries because the engine has no XR device.</summary>
internal static class SubsystemTests
{
    private sealed class FakeSubsystem : IntegratedSubsystem<FakeDescriptor> { }

    private sealed class FakeDescriptor : IntegratedSubsystemDescriptor<FakeSubsystem> { }

    public static void Run()
    {
        QueriesReturnNothing();
        QueriesClearTheCallersList();
        NullListIsRejected();
        EventsAcceptHandlersAndNeverFire();
        SubsystemWithoutDevice();
        DescriptorWithoutDevice();
        Console.WriteLine("PASS: SubsystemManager reports no subsystems, descriptors cannot create and subsystems cannot start");
    }

    private static void QueriesReturnNothing()
    {
        var subsystems = new List<FakeSubsystem>();
        SubsystemManager.GetSubsystems(subsystems);
        Expect.Equal(0, subsystems.Count, "GetSubsystems");
        SubsystemManager.GetInstances(subsystems);
        Expect.Equal(0, subsystems.Count, "GetInstances");
        var descriptors = new List<FakeDescriptor>();
        SubsystemManager.GetSubsystemDescriptors(descriptors);
        Expect.Equal(0, descriptors.Count, "GetSubsystemDescriptors");
        var all = new List<ISubsystemDescriptor>();
        SubsystemManager.GetAllSubsystemDescriptors(all);
        Expect.Equal(0, all.Count, "GetAllSubsystemDescriptors");
    }

    private static void QueriesClearTheCallersList()
    {
        var subsystems = new List<ISubsystem> { new FakeSubsystem() };
        SubsystemManager.GetSubsystems(subsystems);
        Expect.Equal(0, subsystems.Count, "GetSubsystems clears");
        subsystems.Add(new FakeSubsystem());
        SubsystemManager.GetInstances(subsystems);
        Expect.Equal(0, subsystems.Count, "GetInstances clears");
        var descriptors = new List<ISubsystemDescriptor> { new FakeDescriptor() };
        SubsystemManager.GetSubsystemDescriptors(descriptors);
        Expect.Equal(0, descriptors.Count, "GetSubsystemDescriptors clears");
        descriptors.Add(new FakeDescriptor());
        SubsystemManager.GetAllSubsystemDescriptors(descriptors);
        Expect.Equal(0, descriptors.Count, "GetAllSubsystemDescriptors clears");
    }

    private static void NullListIsRejected()
    {
        Expect.Throws<ArgumentNullException>(() => SubsystemManager.GetSubsystems<ISubsystem>(null!), "GetSubsystems null");
        Expect.Throws<ArgumentNullException>(() => SubsystemManager.GetAllSubsystemDescriptors(null!), "GetAllSubsystemDescriptors null");
    }

    private static void EventsAcceptHandlersAndNeverFire()
    {
        var fired = 0;
        void Handler() => fired++;
        SubsystemManager.beforeReloadSubsystems += Handler;
        SubsystemManager.afterReloadSubsystems += Handler;
        SubsystemManager.reloadSubsytemsStarted += Handler;
        SubsystemManager.reloadSubsytemsCompleted += Handler;
        SubsystemManager.GetSubsystems(new List<ISubsystem>());
        SubsystemManager.beforeReloadSubsystems -= Handler;
        SubsystemManager.afterReloadSubsystems -= Handler;
        SubsystemManager.reloadSubsytemsStarted -= Handler;
        SubsystemManager.reloadSubsytemsCompleted -= Handler;
        Expect.Equal(0, fired, "nothing reloads, so no handler runs");
    }

    private static void SubsystemWithoutDevice()
    {
        var subsystem = new FakeSubsystem();
        Expect.That(!subsystem.running, "a subsystem is not running");
        var error = Expect.Catch<NotSupportedException>(subsystem.Start, "Start");
        Expect.That(error.Message.Contains("FakeSubsystem.Start") && error.Message.Contains("no XR device"), error.Message);
        subsystem.Stop();
        subsystem.Destroy();
        Expect.That(!subsystem.running, "Stop and Destroy leave it stopped");
    }

    private static void DescriptorWithoutDevice()
    {
        var descriptor = new FakeDescriptor();
        Expect.Equal(string.Empty, descriptor.id, "id");
        var created = Expect.Catch<NotSupportedException>(() => descriptor.Create(), "Create");
        Expect.That(created.Message.Contains("FakeDescriptor.Create"), created.Message);
        var viaInterface = Expect.Catch<NotSupportedException>(() => ((ISubsystemDescriptor)descriptor).Create(), "Create through the interface");
        Expect.That(viaInterface.Message.Contains("FakeDescriptor.Create"), viaInterface.Message);
        var named = Expect.Catch<NotSupportedException>(() => _ = new FakeSubsystem().subsystemDescriptor, "subsystemDescriptor");
        Expect.That(named.Message.Contains("subsystemDescriptor"), named.Message);
    }
}
