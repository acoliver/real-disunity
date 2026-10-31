using Disunity.Hosting;
using UnityEngine;

/// <summary>ParticleSystem and ParticleSystemRenderer: module values, play state, the absence of simulation,
/// particles set by hand, MinMaxCurve and MinMaxGradient evaluation, attaching and cloning.</summary>
internal static class ParticleSystemTests
{
    public static void Run()
    {
        Values();
        Console.WriteLine("PASS: MinMaxCurve and MinMaxGradient evaluate constants, two constants, curves and gradients");
        DisunityHost.LoadWorld();
        try
        {
            ModuleValues();
            Console.WriteLine("PASS: module values read back as written, defaults are set, and a module copy writes through");
            PlayState();
            Console.WriteLine("PASS: Play, Pause, Stop, Clear and Simulate move the play state and time");
            NoParticlesAreEmitted();
            Console.WriteLine("PASS: Emit makes no particles and GetParticles returns 0; SetParticles particles come back");
            ParticleMembers();
            Console.WriteLine("PASS: Particle members, GetCurrentSize and GetCurrentColor follow the size and color modules");
            Attach();
            Console.WriteLine("PASS: AddComponent<ParticleSystem> brings its renderer, once; the renderer stores its settings");
            Cloning();
            Console.WriteLine("PASS: Instantiate copies module values, particles and renderer settings");
            Children();
            Console.WriteLine("PASS: the withChildren overloads reach child systems and false leaves them");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static ParticleSystem NewSystem() => new GameObject("particles").AddComponent<ParticleSystem>();

    private static void Values()
    {
        ParticleSystem.MinMaxCurve constant = 3f;
        Expect.Equal(ParticleSystemCurveMode.Constant, constant.mode, "a float makes a constant");
        Expect.Near(3f, constant.Evaluate(0.5f), "constant");
        var two = new ParticleSystem.MinMaxCurve(2f, 6f);
        Expect.Equal(ParticleSystemCurveMode.TwoConstants, two.mode, "two constants mode");
        Expect.Near(2f, two.Evaluate(0f, 0f), "two constants at 0");
        Expect.Near(6f, two.Evaluate(0f, 1f), "two constants at 1");
        Expect.Near(3f, two.Evaluate(0f, 0.25f), "two constants at a quarter");
        Expect.Near(2f, two.constantMin, "constantMin");
        Expect.Near(6f, two.constantMax, "constantMax");
        var ramp = new ParticleSystem.MinMaxCurve(10f, AnimationCurve.Linear(0f, 0f, 1f, 1f));
        Expect.Near(5f, ramp.Evaluate(0.5f), "a curve is scaled by its multiplier");
        Expect.That(ramp.curve is not null && ramp.curveMin is null, "a curve has curve and no curveMin");
        var pair = new ParticleSystem.MinMaxCurve(1f, AnimationCurve.Constant(0f, 1f, 2f), AnimationCurve.Constant(0f, 1f, 4f));
        Expect.Near(3f, pair.Evaluate(0.3f, 0.5f), "two curves lerp");
        Expect.That(pair.curveMin is not null && pair.curveMax is not null && pair.curve is null, "two curves have both curves");

        ParticleSystem.MinMaxGradient color = Color.red;
        Expect.Equal(ParticleSystemGradientMode.Color, color.mode, "a color makes a color mode");
        Expect.That(color.Evaluate(0.7f) == Color.red, "a color evaluates to itself");
        var colors = new ParticleSystem.MinMaxGradient(Color.black, Color.white);
        Expect.Near(0.5f, colors.Evaluate(0f, 0.5f).r, "two colors lerp");
        var gradient = new Gradient
        {
            colorKeys = [new GradientColorKey(Color.black, 0f), new GradientColorKey(Color.white, 1f)],
        };
        ParticleSystem.MinMaxGradient fromGradient = gradient;
        Expect.Equal(ParticleSystemGradientMode.Gradient, fromGradient.mode, "a gradient makes a gradient mode");
        Expect.Near(0.25f, fromGradient.Evaluate(0.25f).r, "a gradient evaluates at the time");
        Expect.That(fromGradient.gradient is not null && fromGradient.gradientMin is null, "gradient and no gradientMin");
    }

    private static void ModuleValues()
    {
        var system = NewSystem();
        var main = system.main;
        Expect.Near(5f, main.duration, "default duration");
        Expect.That(main.loop && main.playOnAwake && !main.prewarm, "default switches");
        Expect.Equal(1000, main.maxParticles, "default maxParticles");
        Expect.Equal(ParticleSystemSimulationSpace.Local, main.simulationSpace, "default simulation space");
        Expect.Equal(ParticleSystemScalingMode.Local, main.scalingMode, "default scaling mode");
        main.maxParticles = 14;
        main.duration = 2.5f;
        main.simulationSpace = ParticleSystemSimulationSpace.World;
        main.scalingMode = ParticleSystemScalingMode.Hierarchy;
        main.startLifetime = new ParticleSystem.MinMaxCurve(1f, 3f);
        main.startColor = Color.green;
        Expect.Equal(14, system.main.maxParticles, "a module copy writes through: maxParticles");
        Expect.Near(2.5f, system.main.duration, "a module copy writes through: duration");
        Expect.Equal(ParticleSystemSimulationSpace.World, system.main.simulationSpace, "simulationSpace");
        Expect.Equal(ParticleSystemScalingMode.Hierarchy, system.main.scalingMode, "scalingMode");
        Expect.Near(3f, system.main.startLifetime.constantMax, "startLifetime");
        Expect.That(system.main.startColor.color == Color.green, "startColor");
        main.maxParticles = -5;
        Expect.Equal(0, system.main.maxParticles, "a negative maxParticles is stored as 0");

        var emission = system.emission;
        Expect.That(emission.enabled, "emission is on at first");
        emission.enabled = false;
        Expect.That(!system.emission.enabled, "emission.enabled writes through");

        var sheet = system.textureSheetAnimation;
        Expect.That(!sheet.enabled && sheet.numTilesX == 1 && sheet.numTilesY == 1, "sheet defaults");
        Expect.Near(0.5f, sheet.frameOverTime.Evaluate(0.5f), "the default frame curve is linear");
        sheet.enabled = true;
        sheet.numTilesX = 4;
        sheet.numTilesY = 2;
        sheet.animation = ParticleSystemAnimationType.SingleRow;
        sheet.rowIndex = 1;
        sheet.cycleCount = 3f;
        var read = system.textureSheetAnimation;
        Expect.That(read.enabled && read.numTilesX == 4 && read.numTilesY == 2 && read.rowIndex == 1, "sheet values");
        Expect.Equal(ParticleSystemAnimationType.SingleRow, read.animation, "sheet animation");
        Expect.Near(3f, read.cycleCount, "sheet cycleCount");

        var (colorModule, sizeModule, noiseModule, trailModule) = (system.colorOverLifetime, system.sizeOverLifetime, system.noise, system.trails);
        var (dataModule, shapeModule, velocityModule) = (system.customData, system.shape, system.velocityOverLifetime);
        (colorModule.enabled, sizeModule.enabled, noiseModule.enabled, trailModule.enabled) = (true, true, true, true);
        (dataModule.enabled, velocityModule.enabled, shapeModule.radius) = (true, true, 4f);
        Expect.That(system.colorOverLifetime.enabled && system.sizeOverLifetime.enabled && system.noise.enabled
            && system.trails.enabled && system.customData.enabled && system.velocityOverLifetime.enabled, "module enabled flags");
        Expect.Near(4f, system.shape.radius, "shape radius");
        var other = NewSystem();
        Expect.That(!other.noise.enabled && other.main.maxParticles == 1000, "another system has its own values");
    }

    private static void PlayState()
    {
        var system = NewSystem();
        Expect.That(system.isStopped && !system.isPlaying && !system.isPaused && !system.isEmitting, "a new system is stopped");
        Expect.That(!system.IsAlive(), "a stopped, empty system is not alive");
        system.Play();
        Expect.That(system.isPlaying && system.isEmitting && !system.isStopped && !system.isPaused, "Play");
        Expect.That(system.IsAlive(), "a playing system is alive");
        system.Simulate(1.5f, false, false);
        Expect.Near(1.5f, system.time, "Simulate advances time");
        Expect.That(system.isPlaying, "Simulate leaves the play state");
        system.Pause();
        Expect.That(system.isPaused && !system.isPlaying && !system.isEmitting && !system.isStopped, "Pause");
        system.Play();
        Expect.That(system.isPlaying, "Play resumes a paused system");
        Expect.Near(1.5f, system.time, "resuming keeps the time");
        system.Stop();
        Expect.That(system.isStopped && !system.isPlaying, "Stop");
        system.Pause();
        Expect.That(system.isStopped, "Pause leaves a stopped system");
        system.Play();
        Expect.Near(0f, system.time, "Play from stopped starts at time 0");
        system.Simulate(2f, false, false);
        system.Simulate(1f, false, true);
        Expect.Near(1f, system.time, "Simulate with restart starts again from 0");
        system.time = 4f;
        Expect.Near(4f, system.time, "time is settable");
        system.Simulate(2f, false, false);
        Expect.Near(1f, system.time, "a looping system wraps at its duration");
        var main = system.main;
        main.loop = false;
        system.Simulate(10f, false, false);
        Expect.Near(5f, system.time, "a system that does not loop stops at its duration");
        Expect.Throws<ArgumentOutOfRangeException>(() => system.Simulate(-1f), "negative simulate");
        Expect.Throws<ArgumentOutOfRangeException>(() => system.Stop(true, (ParticleSystemStopBehavior)9), "unknown stop behavior");
    }

    private static ParticleSystem.Particle[] Sample(int count) => Enumerable.Range(0, count).Select(i =>
    {
        var p = new ParticleSystem.Particle { startLifetime = 2f, remainingLifetime = 1f };
        p.position = new Vector3(i, 0f, 0f);
        return p;
    }).ToArray();

    private static void NoParticlesAreEmitted()
    {
        var system = NewSystem();
        system.Play();
        system.Emit(25);
        system.Simulate(3f, false, false);
        Expect.Equal(0, system.particleCount, "nothing is emitted");
        var buffer = Sample(4);
        Expect.Equal(0, system.GetParticles(buffer), "GetParticles returns 0");
        Expect.Near(3f, buffer[3].position.x, "the buffer is left as it was");
        Expect.Throws<ArgumentOutOfRangeException>(() => system.Emit(-1), "negative emit");

        system.SetParticles(Sample(3));
        Expect.Equal(3, system.particleCount, "SetParticles stores the given particles");
        var back = new ParticleSystem.Particle[5];
        Expect.Equal(3, system.GetParticles(back), "GetParticles returns them");
        Expect.Near(2f, back[2].position.x, "the third particle");
        Expect.Equal(2, system.GetParticles(back, 2), "GetParticles honors size");
        Expect.Equal(1, system.GetParticles(back, 5, 2), "GetParticles honors offset");
        Expect.Near(2f, back[0].position.x, "the offset particle lands first");
        Expect.That(system.IsAlive(), "a stopped system with particles is alive");
        system.Simulate(2f, false, false);
        Expect.Equal(3, system.particleCount, "particles do not age");
        system.SetParticles(Sample(2), 2, 1);
        Expect.Equal(3, system.particleCount, "an offset keeps the first particles and appends");
        var main = system.main;
        main.maxParticles = 2;
        system.SetParticles(Sample(5));
        Expect.Equal(2, system.particleCount, "maxParticles caps the stored particles");
        Expect.Throws<ArgumentOutOfRangeException>(() => system.SetParticles(Sample(1), 1, 5), "offset past the count");
        system.Clear();
        Expect.That(system.particleCount == 0 && system.isPlaying, "Clear removes particles and keeps the state");
        system.SetParticles(Sample(2));
        system.Stop();
        Expect.Equal(2, system.particleCount, "Stop keeps particles");
        system.Stop(true, ParticleSystemStopBehavior.StopEmittingAndClear);
        Expect.Equal(0, system.particleCount, "StopEmittingAndClear removes them");
        Expect.Throws<ArgumentNullException>(() => system.GetParticles(null!), "null array");
    }

    private static void ParticleMembers()
    {
        var particle = new ParticleSystem.Particle();
        Expect.That(particle.startSize3D == Vector3.one && particle.startSize == 1f, "a new particle has size 1");
        Expect.That(particle.startColor.r == 255 && particle.startColor.a == 255, "a new particle is white");
        particle.startSize = 2f;
        particle.rotation = 30f;
        particle.angularVelocity = 5f;
        particle.velocity = new Vector3(1f, 2f, 3f);
        particle.randomSeed = 77;
        Expect.That(particle.startSize3D == new Vector3(2f, 2f, 2f), "startSize sets all axes");
        Expect.Near(30f, particle.rotation3D.z, "rotation is the z rotation");
        Expect.Near(5f, particle.angularVelocity3D.z, "angularVelocity is the z angular velocity");
        Expect.That(particle.totalVelocity == new Vector3(1f, 2f, 3f) && particle.randomSeed == 77, "velocity and seed");
        particle.startColor = new Color32(200, 100, 0, 255);
        var system = NewSystem();
        particle.startLifetime = 4f;
        particle.remainingLifetime = 2f;
        Expect.Near(2f, particle.GetCurrentSize(system), "size without the module is the start size");
        Expect.That(particle.GetCurrentColor(system).r == 200, "color without the module is the start color");
        var size = system.sizeOverLifetime;
        size.enabled = true;
        size.size = new ParticleSystem.MinMaxCurve(1f, AnimationCurve.Linear(0f, 0f, 1f, 1f));
        Expect.Near(1f, particle.GetCurrentSize(system), "size at half age is half the start size");
        var color = system.colorOverLifetime;
        color.enabled = true;
        color.color = new Gradient
        {
            colorKeys = [new GradientColorKey(Color.white, 0f), new GradientColorKey(Color.black, 1f)],
        };
        var current = particle.GetCurrentColor(system);
        Expect.That(Math.Abs(current.r - 100) <= 1 && current.g <= 51 && current.g >= 49, $"color at half age is half the start color, got {current}");
        Expect.Throws<ArgumentNullException>(() => particle.GetCurrentSize(null!), "null system");
    }

    private static void Attach()
    {
        var owner = new GameObject("fx");
        var system = owner.AddComponent<ParticleSystem>();
        var renderer = owner.GetComponent<ParticleSystemRenderer>();
        Expect.That(renderer is not null, "the renderer is added with the system");
        Expect.Equal(1, owner.GetComponents<ParticleSystemRenderer>().Length, "one renderer");
        Expect.That(ReferenceEquals(system.GetComponent<Renderer>(), renderer), "the renderer is found as a Renderer");
        Expect.That(renderer!.enabled, "the renderer is enabled");
        renderer.enabled = false;
        renderer.sortingOrder = 3;
        Expect.That(!renderer.enabled && renderer.sortingOrder == 3, "common renderer settings round trip");
        Expect.Equal(ParticleSystemRenderMode.Billboard, renderer.renderMode, "default render mode");
        Expect.Near(0.5f, renderer.maxParticleSize, "default maxParticleSize");
        renderer.maxParticleSize = 0.25f;
        renderer.renderMode = ParticleSystemRenderMode.Stretch;
        renderer.alignment = ParticleSystemRenderSpace.World;
        renderer.sortingFudge = 2f;
        Expect.Near(0.25f, renderer.maxParticleSize, "maxParticleSize");
        Expect.Equal(ParticleSystemRenderMode.Stretch, renderer.renderMode, "renderMode");
        Expect.Equal(ParticleSystemRenderSpace.World, renderer.alignment, "alignment");
        Expect.Near(2f, renderer.sortingFudge, "sortingFudge");
        Expect.That(renderer.GetType() == typeof(ParticleSystemRenderer), "the renderer has its own type");

        var first = new GameObject("renderer first");
        var early = first.AddComponent<ParticleSystemRenderer>();
        first.AddComponent<ParticleSystem>();
        Expect.That(ReferenceEquals(first.GetComponent<ParticleSystemRenderer>(), early), "an existing renderer is kept");
        Expect.Equal(1, first.GetComponents<ParticleSystemRenderer>().Length, "still one renderer");
        Expect.Equal(1, owner.GetComponents<ParticleSystem>().Length, "one system");
    }

    private static void Cloning()
    {
        var owner = new GameObject("original");
        var system = owner.AddComponent<ParticleSystem>();
        var originalMain = system.main;
        originalMain.duration = 7f;
        originalMain.maxParticles = 33;
        system.SetParticles(Sample(2));
        var renderer = owner.GetComponent<ParticleSystemRenderer>()!;
        renderer.maxParticleSize = 0.75f;
        renderer.sortingOrder = 9;
        var clone = UnityEngine.Object.Instantiate(owner);
        var copy = clone.GetComponent<ParticleSystem>()!;
        Expect.That(!ReferenceEquals(copy, system), "the clone has its own system");
        Expect.Near(7f, copy.main.duration, "the clone has the duration");
        Expect.Equal(33, copy.main.maxParticles, "the clone has maxParticles");
        Expect.Equal(2, copy.particleCount, "the clone has the particles");
        var copyMain = copy.main;
        copyMain.duration = 1f;
        Expect.Near(7f, system.main.duration, "the clone's values are its own");
        var copyRenderer = clone.GetComponent<ParticleSystemRenderer>()!;
        Expect.Near(0.75f, copyRenderer.maxParticleSize, "the clone's renderer has the particle settings");
        Expect.Equal(9, copyRenderer.sortingOrder, "the clone's renderer has the common settings");
    }

    private static void Children()
    {
        var parent = NewSystem();
        var child = new GameObject("child");
        child.transform.SetParent(parent.transform, false);
        var inner = child.AddComponent<ParticleSystem>();
        parent.Play(false);
        Expect.That(parent.isPlaying && inner.isStopped, "Play(false) leaves the child");
        parent.Play();
        Expect.That(inner.isPlaying, "Play reaches the child");
        parent.Pause(false);
        Expect.That(parent.isPaused && inner.isPlaying, "Pause(false) leaves the child");
        parent.Stop(true);
        Expect.That(parent.isStopped && inner.isStopped, "Stop(true) reaches the child");
        inner.SetParticles(Sample(1));
        Expect.That(parent.IsAlive(true) && !parent.IsAlive(false), "IsAlive(true) sees the child's particles and false does not");
        parent.Clear(false);
        Expect.Equal(1, inner.particleCount, "Clear(false) leaves the child");
        parent.Clear();
        Expect.Equal(0, inner.particleCount, "Clear reaches the child");
        parent.Simulate(2f, true, true, true);
        Expect.Near(2f, inner.time, "Simulate reaches the child");
    }
}
