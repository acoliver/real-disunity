using Disunity.Hosting;

namespace UnityEngine;

/// <summary>The base of the engine objects a coroutine can yield to wait on.</summary>
public class YieldInstruction;

/// <summary>
/// Work the engine finishes on the main thread. A coroutine that yields one resumes in the
/// first frame after it is done. <see cref="completed"/> handlers added before it finishes run
/// when it finishes; one added afterwards runs at once.
/// </summary>
public class AsyncOperation : YieldInstruction
{
    private Action<AsyncOperation>? _completed;
    private float _progress;

    public bool isDone { get; private set; }
    public float progress => isDone ? 1 : _progress;
    public int priority { get; set; }
    public bool allowSceneActivation { get; set; } = true;

    public event Action<AsyncOperation> completed
    {
        add
        {
            if (isDone) value(this);
            else _completed += value;
        }
        remove => _completed -= value;
    }

    internal void Report(float progress) => _progress = Math.Clamp(progress, 0, 0.9f);

    internal void Complete()
    {
        if (isDone) return;
        isDone = true;
        var handlers = _completed;
        _completed = null;
        handlers?.Invoke(this);
    }

    /// <summary>Completes in the next frame's delayed tasks, as Unity completes work that
    /// finishes at once, so handlers added in this frame still run.</summary>
    /// <summary>Completes in the next frame's delayed tasks, or at once without a loaded world.</summary>
    internal void CompleteNextFrame()
    {
        if (!DisunityHost.PostToMainThread(Complete)) Complete();
    }
}

/// <summary>A <see cref="Resources.LoadAsync(string)"/> request. Reading <see cref="asset"/>
/// before the request is done loads the asset at once, as in Unity.</summary>
public class ResourceRequest : AsyncOperation
{
    private readonly Func<Object?> _load;
    private Object? _asset;
    private bool _loaded;

    internal ResourceRequest(Func<Object?> load)
    {
        _load = load;
        CompleteNextFrame();
    }

    public Object? asset
    {
        get
        {
            if (!_loaded)
            {
                _asset = _load();
                _loaded = true;
            }
            return _asset;
        }
    }
}
