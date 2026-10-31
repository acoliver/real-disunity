namespace UnityEngine;

/// <summary>
/// A particle system component. The engine's choice for now is that nothing simulates: a system tracks its play
/// state and its <see cref="time"/>, every module value is stored and read back as written, and no particle is
/// ever emitted, so <see cref="Emit(int)"/> adds nothing and <see cref="particleCount"/> is 0. The one way to get
/// particles into a system is <see cref="SetParticles(Particle[], int, int)"/>; they are kept as given, they do not
/// age or move, and <see cref="GetParticles(Particle[], int, int)"/> returns them. The native component marks the
/// object; settings and particles live on the managed component and a clone copies them. A new system is stopped
/// at time 0 whatever <c>main.playOnAwake</c> says. It requires a <see cref="ParticleSystemRenderer"/>, which
/// <see cref="GameObject.AddComponent(Type)"/> adds first.
/// </summary>
[RequireComponent(typeof(ParticleSystemRenderer))]
public sealed partial class ParticleSystem : NativeComponent
{
    private enum PlayState { Stopped, Playing, Paused }

    private readonly ParticleSettings _settings = new();
    private readonly List<Particle> _particles = [];
    private PlayState _state = PlayState.Stopped;
    private float _time;

    public bool isPlaying => _state == PlayState.Playing;

    public bool isPaused => _state == PlayState.Paused;

    public bool isStopped => _state == PlayState.Stopped;

    /// <summary>True while the system is playing: the engine emits nothing, so this only says new particles would be made.</summary>
    public bool isEmitting => _state == PlayState.Playing;

    public int particleCount => _particles.Count;

    /// <summary>Seconds into the system's cycle. A looping system wraps at <c>main.duration</c> and another stops at it.</summary>
    public float time
    {
        get => _time;
        set => _time = Wrap(Math.Max(0f, value));
    }

    private float Wrap(float seconds)
    {
        var duration = main.duration;
        if (duration <= 0f) return 0f;
        return main.loop ? Mathf.Repeat(seconds, duration) : Math.Min(seconds, duration);
    }

    /// <summary>Starts a stopped system from time 0 and resumes a paused one.</summary>
    public void Play() => Play(true);

    public void Play(bool withChildren) => Each(withChildren, system => system.StartPlaying());

    private void StartPlaying()
    {
        if (_state == PlayState.Stopped) _time = 0f;
        _state = PlayState.Playing;
    }

    /// <summary>Stops emitting and keeps the particles.</summary>
    public void Stop() => Stop(true, ParticleSystemStopBehavior.StopEmitting);

    public void Stop(bool withChildren) => Stop(withChildren, ParticleSystemStopBehavior.StopEmitting);

    public void Stop(bool withChildren, ParticleSystemStopBehavior stopBehavior)
    {
        if (!Enum.IsDefined(stopBehavior)) throw new ArgumentOutOfRangeException(nameof(stopBehavior), stopBehavior, "unknown stop behavior");
        Each(withChildren, system => system.StopPlaying(stopBehavior));
    }

    private void StopPlaying(ParticleSystemStopBehavior behavior)
    {
        _state = PlayState.Stopped;
        if (behavior == ParticleSystemStopBehavior.StopEmittingAndClear) _particles.Clear();
    }

    /// <summary>Pauses a playing system; a stopped or paused one stays as it is.</summary>
    public void Pause() => Pause(true);

    public void Pause(bool withChildren) => Each(withChildren, system =>
    {
        if (system._state == PlayState.Playing) system._state = PlayState.Paused;
    });

    /// <summary>Removes the particles and leaves the play state and time.</summary>
    public void Clear() => Clear(true);

    public void Clear(bool withChildren) => Each(withChildren, system => system._particles.Clear());

    public void Simulate(float t) => Simulate(t, true, true, true);

    public void Simulate(float t, bool withChildren) => Simulate(t, withChildren, true, true);

    public void Simulate(float t, bool withChildren, bool restart) => Simulate(t, withChildren, restart, true);

    /// <summary>
    /// Advances <see cref="time"/> by <paramref name="t"/> seconds, from 0 after clearing when
    /// <paramref name="restart"/> is set. The play state does not change. <paramref name="fixedTimeStep"/> has no
    /// effect because nothing is simulated.
    /// </summary>
    public void Simulate(float t, bool withChildren, bool restart, bool fixedTimeStep)
    {
        if (!(t >= 0f)) throw new ArgumentOutOfRangeException(nameof(t), t, "the simulated time is not negative");
        Each(withChildren, system =>
        {
            if (restart)
            {
                system._particles.Clear();
                system._time = 0f;
            }
            system._time = system.Wrap(system._time + t);
        });
    }

    /// <summary>Emits no particles: the engine does not simulate. A negative count throws.</summary>
    public void Emit(int count)
    {
        if (count < 0) throw new ArgumentOutOfRangeException(nameof(count), count, "the emit count is not negative");
    }

    /// <summary>True while the system plays or is paused, or holds particles.</summary>
    public bool IsAlive() => IsAlive(true);

    public bool IsAlive(bool withChildren) => Children(withChildren).Any(system => system._state != PlayState.Stopped || system._particles.Count > 0);

    public int GetParticles(Particle[] particles) => GetParticles(particles, particles?.Length ?? 0, 0);

    public int GetParticles(Particle[] particles, int size) => GetParticles(particles, size, 0);

    /// <summary>Copies up to <paramref name="size"/> of the system's particles, starting at the system's particle
    /// <paramref name="offset"/>, to the start of <paramref name="particles"/>, and returns how many it copied.</summary>
    public int GetParticles(Particle[] particles, int size, int offset)
    {
        ArgumentNullException.ThrowIfNull(particles);
        CheckRange(size, offset);
        var count = Math.Max(0, Math.Min(Math.Min(size, particles.Length), _particles.Count - offset));
        _particles.CopyTo(offset, particles, 0, count);
        return count;
    }

    public void SetParticles(Particle[] particles) => SetParticles(particles, particles?.Length ?? 0, 0);

    public void SetParticles(Particle[] particles, int size) => SetParticles(particles, size, 0);

    /// <summary>Keeps the system's first <paramref name="offset"/> particles and follows them with the first
    /// <paramref name="size"/> of <paramref name="particles"/>, up to <c>main.maxParticles</c> in all. An offset past
    /// the particle count throws.</summary>
    public void SetParticles(Particle[] particles, int size, int offset)
    {
        ArgumentNullException.ThrowIfNull(particles);
        CheckRange(size, offset);
        if (size > particles.Length) throw new ArgumentException("the size is larger than the array", nameof(size));
        if (offset > _particles.Count) throw new ArgumentOutOfRangeException(nameof(offset), offset, "the offset is past the particle count");
        _particles.RemoveRange(offset, _particles.Count - offset);
        _particles.AddRange(particles.Take(size));
        var limit = main.maxParticles;
        if (_particles.Count > limit) _particles.RemoveRange(limit, _particles.Count - limit);
    }

    private static void CheckRange(int size, int offset)
    {
        if (size < 0) throw new ArgumentOutOfRangeException(nameof(size), size, "the size is not negative");
        if (offset < 0) throw new ArgumentOutOfRangeException(nameof(offset), offset, "the offset is not negative");
    }

    private IEnumerable<ParticleSystem> Children(bool withChildren) =>
        withChildren ? GetComponentsInChildren<ParticleSystem>(true) : [this];

    private void Each(bool withChildren, Action<ParticleSystem> action)
    {
        foreach (var system in Children(withChildren)) action(system);
    }

    internal override void CopyManagedState(NativeComponent source, Func<Object, Object> remap)
    {
        var original = (ParticleSystem)source;
        _settings.CopyFrom(original._settings);
        _particles.AddRange(original._particles);
        (_state, _time) = (original._state, original._time);
    }

    public MainModule main => new(_settings);

    public EmissionModule emission => new(_settings);

    public ShapeModule shape => new(_settings);

    public VelocityOverLifetimeModule velocityOverLifetime => new(_settings);

    public ColorOverLifetimeModule colorOverLifetime => new(_settings);

    public SizeOverLifetimeModule sizeOverLifetime => new(_settings);

    public NoiseModule noise => new(_settings);

    public TrailModule trails => new(_settings);

    public CustomDataModule customData => new(_settings);

    public TextureSheetAnimationModule textureSheetAnimation => new(_settings);
}
