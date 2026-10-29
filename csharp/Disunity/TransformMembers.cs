namespace UnityEngine;

/// <summary>Axes, Euler angles, matrices, space conversion and the relative moves of a Transform,
/// built from the native local position, rotation and scale.</summary>
public partial class Transform
{
    public Vector3 right { get => rotation * Vector3.right; set => rotation = Quaternion.FromToRotation(Vector3.right, value); }
    public Vector3 up { get => rotation * Vector3.up; set => rotation = Quaternion.FromToRotation(Vector3.up, value); }
    public Vector3 forward { get => rotation * Vector3.forward; set => rotation = Quaternion.LookRotation(value); }

    public Vector3 eulerAngles { get => rotation.eulerAngles; set => rotation = Quaternion.Euler(value); }
    public Vector3 localEulerAngles { get => localRotation.eulerAngles; set => localRotation = Quaternion.Euler(value); }

    private Matrix4x4 LocalMatrix()
    {
        var local = ReadLocal();
        return Matrix4x4.TRS(new Vector3(local.x, local.y, local.z), new Quaternion(local.qx, local.qy, local.qz, local.qw),
            new Vector3(local.sx, local.sy, local.sz));
    }

    /// <summary>The chain of the local position, rotation and scale of this transform and its ancestors.</summary>
    public Matrix4x4 localToWorldMatrix
    {
        get
        {
            var matrix = LocalMatrix();
            for (var ancestor = parent; ancestor is not null; ancestor = ancestor.parent) matrix = ancestor.LocalMatrix() * matrix;
            return matrix;
        }
    }

    public Matrix4x4 worldToLocalMatrix => localToWorldMatrix.inverse;

    /// <summary>The lengths of the world matrix's axes, which is Unity's approximation when a rotated parent is scaled unevenly.</summary>
    public Vector3 lossyScale => localToWorldMatrix.lossyScale;

    public Vector3 TransformPoint(Vector3 position) => localToWorldMatrix.MultiplyPoint3x4(position);
    public Vector3 TransformPoint(float x, float y, float z) => TransformPoint(new Vector3(x, y, z));
    public Vector3 InverseTransformPoint(Vector3 position) => worldToLocalMatrix.MultiplyPoint3x4(position);
    public Vector3 InverseTransformPoint(float x, float y, float z) => InverseTransformPoint(new Vector3(x, y, z));

    /// <summary>Rotates a direction by this transform's rotation; scale does not apply.</summary>
    public Vector3 TransformDirection(Vector3 direction) => rotation * direction;
    public Vector3 TransformDirection(float x, float y, float z) => TransformDirection(new Vector3(x, y, z));
    public Vector3 InverseTransformDirection(Vector3 direction) => Quaternion.Inverse(rotation) * direction;
    public Vector3 InverseTransformDirection(float x, float y, float z) => InverseTransformDirection(new Vector3(x, y, z));

    /// <summary>Rotates and scales a vector; the position does not apply.</summary>
    public Vector3 TransformVector(Vector3 vector) => localToWorldMatrix.MultiplyVector(vector);
    public Vector3 TransformVector(float x, float y, float z) => TransformVector(new Vector3(x, y, z));
    public Vector3 InverseTransformVector(Vector3 vector) => worldToLocalMatrix.MultiplyVector(vector);
    public Vector3 InverseTransformVector(float x, float y, float z) => InverseTransformVector(new Vector3(x, y, z));

    public void SetPositionAndRotation(Vector3 position, Quaternion rotation)
    {
        this.position = position;
        this.rotation = rotation;
    }

    public void SetLocalPositionAndRotation(Vector3 localPosition, Quaternion localRotation)
    {
        this.localPosition = localPosition;
        this.localRotation = localRotation;
    }

    public void GetPositionAndRotation(out Vector3 position, out Quaternion rotation)
    {
        position = this.position;
        rotation = this.rotation;
    }

    public void GetLocalPositionAndRotation(out Vector3 localPosition, out Quaternion localRotation)
    {
        localPosition = this.localPosition;
        localRotation = this.localRotation;
    }

    public void Translate(Vector3 translation) => Translate(translation, Space.Self);
    public void Translate(float x, float y, float z) => Translate(new Vector3(x, y, z), Space.Self);
    public void Translate(float x, float y, float z, Space relativeTo) => Translate(new Vector3(x, y, z), relativeTo);

    /// <summary>Moves by <paramref name="translation"/> along this transform's axes (Space.Self, without its scale) or the world's.</summary>
    public void Translate(Vector3 translation, Space relativeTo) =>
        position += relativeTo == Space.World ? translation : TransformDirection(translation);

    public void Translate(float x, float y, float z, Transform? relativeTo) => Translate(new Vector3(x, y, z), relativeTo);

    /// <summary>Moves along the axes of <paramref name="relativeTo"/>; null means the world's axes.</summary>
    public void Translate(Vector3 translation, Transform? relativeTo) =>
        position += relativeTo is null ? translation : relativeTo.TransformDirection(translation);

    public void Rotate(Vector3 eulers) => Rotate(eulers, Space.Self);
    public void Rotate(float xAngle, float yAngle, float zAngle) => Rotate(new Vector3(xAngle, yAngle, zAngle), Space.Self);
    public void Rotate(float xAngle, float yAngle, float zAngle, Space relativeTo) => Rotate(new Vector3(xAngle, yAngle, zAngle), relativeTo);
    public void Rotate(Vector3 eulers, Space relativeTo) => ApplyRotation(Quaternion.Euler(eulers), relativeTo);
    public void Rotate(Vector3 axis, float angle) => Rotate(axis, angle, Space.Self);

    /// <summary>Turns by <paramref name="angle"/> degrees around an axis given in this transform's or the world's space.</summary>
    public void Rotate(Vector3 axis, float angle, Space relativeTo) =>
        ApplyRotation(Quaternion.AngleAxis(angle, axis.normalized), relativeTo);

    private void ApplyRotation(Quaternion turn, Space relativeTo)
    {
        if (relativeTo == Space.World) rotation = turn * rotation;
        else localRotation = localRotation * turn;
    }

    /// <summary>Orbits the position around <paramref name="point"/> and turns the rotation about the same world axis.</summary>
    public void RotateAround(Vector3 point, Vector3 axis, float angle)
    {
        var turn = Quaternion.AngleAxis(angle, axis.normalized);
        position = point + turn * (position - point);
        rotation = turn * rotation;
    }

    public void RotateAround(Vector3 axis, float angle) => rotation = Quaternion.AngleAxis(angle, axis.normalized) * rotation;

    public void LookAt(Transform target) => LookAt(target, Vector3.up);
    public void LookAt(Transform target, Vector3 worldUp) => LookAt(target.position, worldUp);
    public void LookAt(Vector3 worldPosition) => LookAt(worldPosition, Vector3.up);

    /// <summary>Faces <paramref name="worldPosition"/>; nothing happens at the transform's own position.</summary>
    public void LookAt(Vector3 worldPosition, Vector3 worldUp)
    {
        var direction = worldPosition - position;
        if (direction.sqrMagnitude < 1e-12f) return;
        rotation = Quaternion.LookRotation(direction, worldUp);
    }
}
