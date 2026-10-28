using System.Runtime.CompilerServices;
using System.Threading;
using Disunity.Hosting;

namespace UnityEngine.SceneManagement
{
    /// <summary>A loaded scene. The engine has one scene and no scene manager yet, so a value is
    /// only the default (invalid) scene; the scene members arrive with scene management.</summary>
    public readonly struct Scene : IEquatable<Scene>
    {
        internal int Handle { get; init; }

        public bool IsValid() => Handle != 0;

        public static bool operator ==(Scene lhs, Scene rhs) => lhs.Handle == rhs.Handle;

        public static bool operator !=(Scene lhs, Scene rhs) => lhs.Handle != rhs.Handle;

        public bool Equals(Scene other) => Handle == other.Handle;

        public override bool Equals(object? other) => other is Scene scene && Equals(scene);

        public override int GetHashCode() => Handle;
    }
}

namespace UnityEngine
{
    using UnityEngine.SceneManagement;

    /// <summary>Where and how <see cref="Object.Instantiate{T}(T, InstantiateParameters)"/> places a clone.</summary>
    public struct InstantiateParameters
    {
        public Transform parent;
        public Scene scene;
        public bool worldSpace;
        public bool originalImmutable;
    }

    /// <summary>A batch of clones made when the operation completes, in the first frame after the call,
    /// so the clones do not exist (and scripts cannot find them) before <see cref="AsyncOperation.isDone"/>.</summary>
    public class AsyncInstantiateOperation : AsyncOperation
    {
        private readonly CancellationToken _cancellation;
        private Func<Object[]>? _create;
        private Object[]? _result;

        internal AsyncInstantiateOperation(Func<Object[]> create, CancellationToken cancellation)
        {
            _create = create;
            _cancellation = cancellation;
            CompleteNextFrame();
        }

        public Object[]? Result
        {
            get
            {
                Run();
                return _result;
            }
        }

        public bool IsWaitingForSceneActivation() => false;

        public void WaitForCompletion() => Run();

        /// <summary>Cancelling before the clones exist leaves <see cref="Result"/> empty.</summary>
        public void Cancel()
        {
            _create = null;
            _result ??= [];
            Complete();
        }

        public static float GetIntegrationTimeMS() => IntegrationTime;

        public static void SetIntegrationTimeMS(float integrationTimeMS) => IntegrationTime = integrationTimeMS;

        private static float IntegrationTime { get; set; } = 2f;

        private protected void Run()
        {
            if (_create is null) return;
            var create = _create;
            _create = null;
            _result = _cancellation.IsCancellationRequested ? [] : create();
            Complete();
        }
    }

    public class AsyncInstantiateOperation<T> : AsyncInstantiateOperation where T : Object
    {
        internal AsyncInstantiateOperation(Func<Object[]> create, CancellationToken cancellation)
            : base(create, cancellation)
        {
        }

        public new T[]? Result => base.Result is { } all ? all.Cast<T>().ToArray() : null;

        public Awaiter GetAwaiter() => new(this);

        public readonly struct Awaiter : INotifyCompletion
        {
            private readonly AsyncInstantiateOperation<T> _op;

            internal Awaiter(AsyncInstantiateOperation<T> op) => _op = op;

            public bool IsCompleted => _op.isDone;

            public void OnCompleted(Action continuation) => _op.completed += _ => continuation();

            public T[]? GetResult() => _op.Result;
        }
    }

    public partial class Object
    {
        public static T Instantiate<T>(T original, InstantiateParameters parameters) where T : Object =>
            Instantiate(original, parameters.parent, parameters.worldSpace);

        public static T Instantiate<T>(T original, Vector3 position, Quaternion rotation, InstantiateParameters parameters)
            where T : Object => Instantiate(original, position, rotation, parameters.parent);

        /// <summary>The engine has one scene and no way to name another, so no scene can be chosen.</summary>
        public static Object Instantiate(Object original, Scene scene) =>
            throw new NotSupportedException("Object.Instantiate(Object, Scene) is not supported yet");

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original) where T : Object =>
            InstantiateAsync(original, 1, default, CancellationToken.None);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, Transform parent) where T : Object =>
            InstantiateAsync(original, 1, new InstantiateParameters { parent = parent }, CancellationToken.None);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, Vector3 position, Quaternion rotation)
            where T : Object => InstantiateAsync(original, 1, position, rotation, default, CancellationToken.None);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, Transform parent, Vector3 position,
            Quaternion rotation) where T : Object =>
            InstantiateAsync(original, 1, position, rotation, new InstantiateParameters { parent = parent }, CancellationToken.None);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count) where T : Object =>
            InstantiateAsync(original, count, default, CancellationToken.None);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count, Transform parent) where T : Object =>
            InstantiateAsync(original, count, new InstantiateParameters { parent = parent }, CancellationToken.None);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count, Vector3 position,
            Quaternion rotation) where T : Object =>
            InstantiateAsync(original, count, position, rotation, default, CancellationToken.None);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count, ReadOnlySpan<Vector3> positions,
            ReadOnlySpan<Quaternion> rotations) where T : Object =>
            InstantiateAsync(original, count, positions, rotations, default, CancellationToken.None);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count, Transform parent,
            Vector3 position, Quaternion rotation) where T : Object =>
            InstantiateAsync(original, count, position, rotation, new InstantiateParameters { parent = parent }, CancellationToken.None);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count, Transform parent,
            Vector3 position, Quaternion rotation, CancellationToken cancellationToken) where T : Object =>
            InstantiateAsync(original, count, position, rotation, new InstantiateParameters { parent = parent }, cancellationToken);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count, Transform parent,
            ReadOnlySpan<Vector3> positions, ReadOnlySpan<Quaternion> rotations) where T : Object =>
            InstantiateAsync(original, count, positions, rotations, new InstantiateParameters { parent = parent },
                CancellationToken.None);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count, Transform parent,
            ReadOnlySpan<Vector3> positions, ReadOnlySpan<Quaternion> rotations, CancellationToken cancellationToken)
            where T : Object =>
            InstantiateAsync(original, count, positions, rotations, new InstantiateParameters { parent = parent },
                cancellationToken);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, InstantiateParameters parameters,
            CancellationToken cancellationToken) where T : Object =>
            InstantiateAsync(original, 1, parameters, cancellationToken);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count,
            InstantiateParameters parameters, CancellationToken cancellationToken) where T : Object
        {
            ValidateBatch(original, count);
            return new(() => CloneBatch(original, count, parameters, null, null), cancellationToken);
        }

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, Vector3 position, Quaternion rotation,
            InstantiateParameters parameters, CancellationToken cancellationToken) where T : Object =>
            InstantiateAsync(original, 1, position, rotation, parameters, cancellationToken);

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count, Vector3 position,
            Quaternion rotation, InstantiateParameters parameters, CancellationToken cancellationToken) where T : Object
        {
            ValidateBatch(original, count);
            return new(() => CloneBatch(original, count, parameters, new[] { position }, new[] { rotation }), cancellationToken);
        }

        public static AsyncInstantiateOperation<T> InstantiateAsync<T>(T original, int count,
            ReadOnlySpan<Vector3> positions, ReadOnlySpan<Quaternion> rotations, InstantiateParameters parameters,
            CancellationToken cancellationToken) where T : Object
        {
            ValidateBatch(original, count);
            if (positions.Length != count || rotations.Length != count)
                throw new ArgumentException("positions and rotations must each have one entry per clone");
            var places = positions.ToArray();
            var turns = rotations.ToArray();
            return new(() => CloneBatch(original, count, parameters, places, turns), cancellationToken);
        }

        private static void ValidateBatch(Object original, int count)
        {
            ArgumentNullException.ThrowIfNull(original);
            ArgumentOutOfRangeException.ThrowIfNegative(count);
        }

        /// <summary>One position and rotation apply to every clone; a list supplies one per clone.</summary>
        private static Object[] CloneBatch<T>(T original, int count, InstantiateParameters parameters,
            Vector3[]? positions, Quaternion[]? rotations) where T : Object
        {
            var clones = new Object[count];
            for (var i = 0; i < count; i++)
            {
                var at = positions is null ? (int?)null : positions.Length == 1 ? 0 : i;
                clones[i] = at is { } index
                    ? Instantiate(original, positions![index], rotations![index], parameters.parent)
                    : Instantiate(original, parameters.parent, parameters.worldSpace);
            }
            return clones;
        }
    }
}
