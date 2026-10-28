using System.Globalization;

namespace UnityEngine;

/// <summary>A 4x4 matrix. The fields are named <c>mRowColumn</c> and stored column by column, so
/// the memory order is <c>m00, m10, m20, m30, m01, ...</c> and a flat index is
/// <c>row + column * 4</c>. Points and vectors are column vectors multiplied on the right. The
/// projection matrices use OpenGL conventions, with the camera looking down negative Z and depth
/// mapped to -1..1. <c>==</c> compares the columns approximately; <c>Equals</c> compares exactly.</summary>
public struct Matrix4x4 : IEquatable<Matrix4x4>, IFormattable
{
    public float m00;
    public float m10;
    public float m20;
    public float m30;
    public float m01;
    public float m11;
    public float m21;
    public float m31;
    public float m02;
    public float m12;
    public float m22;
    public float m32;
    public float m03;
    public float m13;
    public float m23;
    public float m33;

    public Matrix4x4(Vector4 column0, Vector4 column1, Vector4 column2, Vector4 column3)
    {
        m00 = column0.x; m10 = column0.y; m20 = column0.z; m30 = column0.w;
        m01 = column1.x; m11 = column1.y; m21 = column1.z; m31 = column1.w;
        m02 = column2.x; m12 = column2.y; m22 = column2.z; m32 = column2.w;
        m03 = column3.x; m13 = column3.y; m23 = column3.z; m33 = column3.w;
    }

    public Matrix4x4(ref Vector4 column0, ref Vector4 column1, ref Vector4 column2, ref Vector4 column3)
        : this(column0, column1, column2, column3) { }

    public float this[int row, int column]
    {
        readonly get => this[row + column * 4];
        set => this[row + column * 4] = value;
    }

    public float this[int index]
    {
        readonly get => index switch
        {
            0 => m00, 1 => m10, 2 => m20, 3 => m30,
            4 => m01, 5 => m11, 6 => m21, 7 => m31,
            8 => m02, 9 => m12, 10 => m22, 11 => m32,
            12 => m03, 13 => m13, 14 => m23, 15 => m33,
            _ => throw new IndexOutOfRangeException("Invalid matrix index!"),
        };
        set
        {
            switch (index)
            {
                case 0: m00 = value; break;
                case 1: m10 = value; break;
                case 2: m20 = value; break;
                case 3: m30 = value; break;
                case 4: m01 = value; break;
                case 5: m11 = value; break;
                case 6: m21 = value; break;
                case 7: m31 = value; break;
                case 8: m02 = value; break;
                case 9: m12 = value; break;
                case 10: m22 = value; break;
                case 11: m32 = value; break;
                case 12: m03 = value; break;
                case 13: m13 = value; break;
                case 14: m23 = value; break;
                case 15: m33 = value; break;
                default: throw new IndexOutOfRangeException("Invalid matrix index!");
            }
        }
    }

    public static Matrix4x4 zero => new(Vector4.zero, Vector4.zero, Vector4.zero, Vector4.zero);

    public static Matrix4x4 identity => new(
        new Vector4(1f, 0f, 0f, 0f), new Vector4(0f, 1f, 0f, 0f), new Vector4(0f, 0f, 1f, 0f), new Vector4(0f, 0f, 0f, 1f));

    public readonly Vector4 GetColumn(int index) => index switch
    {
        0 => new Vector4(m00, m10, m20, m30),
        1 => new Vector4(m01, m11, m21, m31),
        2 => new Vector4(m02, m12, m22, m32),
        3 => new Vector4(m03, m13, m23, m33),
        _ => throw new IndexOutOfRangeException("Invalid column index!"),
    };

    public readonly Vector4 GetRow(int index) => index switch
    {
        0 => new Vector4(m00, m01, m02, m03),
        1 => new Vector4(m10, m11, m12, m13),
        2 => new Vector4(m20, m21, m22, m23),
        3 => new Vector4(m30, m31, m32, m33),
        _ => throw new IndexOutOfRangeException("Invalid row index!"),
    };

    public readonly Vector3 GetPosition() => new(m03, m13, m23);

    public void SetColumn(int index, Vector4 column)
    {
        this[0, index] = column.x;
        this[1, index] = column.y;
        this[2, index] = column.z;
        this[3, index] = column.w;
    }

    public void SetColumn(int index, ref Vector4 column) => SetColumn(index, column);

    public void SetRow(int index, Vector4 row)
    {
        this[index, 0] = row.x;
        this[index, 1] = row.y;
        this[index, 2] = row.z;
        this[index, 3] = row.w;
    }

    public void SetRow(int index, ref Vector4 row) => SetRow(index, row);

    public static Matrix4x4 operator *(Matrix4x4 lhs, Matrix4x4 rhs)
    {
        Vector4 r0 = lhs.GetRow(0), r1 = lhs.GetRow(1), r2 = lhs.GetRow(2), r3 = lhs.GetRow(3);
        return new Matrix4x4(Product(r0, r1, r2, r3, rhs.GetColumn(0)), Product(r0, r1, r2, r3, rhs.GetColumn(1)),
            Product(r0, r1, r2, r3, rhs.GetColumn(2)), Product(r0, r1, r2, r3, rhs.GetColumn(3)));
    }

    public static Vector4 operator *(Matrix4x4 lhs, Vector4 vector) =>
        Product(lhs.GetRow(0), lhs.GetRow(1), lhs.GetRow(2), lhs.GetRow(3), vector);

    private static Vector4 Product(Vector4 row0, Vector4 row1, Vector4 row2, Vector4 row3, Vector4 column) =>
        new(Vector4.Dot(row0, column), Vector4.Dot(row1, column), Vector4.Dot(row2, column), Vector4.Dot(row3, column));

    public static bool operator ==(Matrix4x4 lhs, Matrix4x4 rhs) =>
        lhs.GetColumn(0) == rhs.GetColumn(0) && lhs.GetColumn(1) == rhs.GetColumn(1)
        && lhs.GetColumn(2) == rhs.GetColumn(2) && lhs.GetColumn(3) == rhs.GetColumn(3);

    public static bool operator !=(Matrix4x4 lhs, Matrix4x4 rhs) => !(lhs == rhs);

    public override readonly int GetHashCode() =>
        GetColumn(0).GetHashCode() ^ (GetColumn(1).GetHashCode() << 2) ^ (GetColumn(2).GetHashCode() >> 2) ^ (GetColumn(3).GetHashCode() >> 1);

    public override readonly bool Equals(object? other) => other is Matrix4x4 matrix && Equals(matrix);

    public readonly bool Equals(Matrix4x4 other) =>
        GetColumn(0).Equals(other.GetColumn(0)) && GetColumn(1).Equals(other.GetColumn(1))
        && GetColumn(2).Equals(other.GetColumn(2)) && GetColumn(3).Equals(other.GetColumn(3));

    public readonly bool Equals(ref Matrix4x4 other) => Equals(other);

    /// <summary>Rotation, then scale, then translation: <c>T * R * S</c>.</summary>
    public static Matrix4x4 TRS(Vector3 pos, Quaternion q, Vector3 s)
    {
        var rotation = Rotate(q);
        return new Matrix4x4(rotation.GetColumn(0) * s.x, rotation.GetColumn(1) * s.y, rotation.GetColumn(2) * s.z,
            new Vector4(pos.x, pos.y, pos.z, 1f));
    }

    public static Matrix4x4 TRS(ref Vector3 pos, ref Quaternion q, ref Vector3 s) => TRS(pos, q, s);
    public void SetTRS(Vector3 pos, Quaternion q, Vector3 s) => this = TRS(pos, q, s);
    public void SetTRS(ref Vector3 pos, ref Quaternion q, ref Vector3 s) => this = TRS(pos, q, s);

    public static Matrix4x4 Translate(Vector3 vector) => new(
        new Vector4(1f, 0f, 0f, 0f), new Vector4(0f, 1f, 0f, 0f), new Vector4(0f, 0f, 1f, 0f), new Vector4(vector.x, vector.y, vector.z, 1f));

    public static Matrix4x4 Translate(ref Vector3 vector) => Translate(vector);

    public static Matrix4x4 Scale(Vector3 vector) => new(
        new Vector4(vector.x, 0f, 0f, 0f), new Vector4(0f, vector.y, 0f, 0f), new Vector4(0f, 0f, vector.z, 0f), new Vector4(0f, 0f, 0f, 1f));

    public static Matrix4x4 Scale(ref Vector3 vector) => Scale(vector);

    /// <summary>The rotation matrix of <paramref name="q"/>, which is expected to be a unit quaternion.</summary>
    public static Matrix4x4 Rotate(Quaternion q)
    {
        float xx = q.x * q.x, yy = q.y * q.y, zz = q.z * q.z;
        float xy = q.x * q.y, xz = q.x * q.z, yz = q.y * q.z;
        float wx = q.w * q.x, wy = q.w * q.y, wz = q.w * q.z;
        return new Matrix4x4(
            new Vector4(1f - 2f * (yy + zz), 2f * (xy + wz), 2f * (xz - wy), 0f),
            new Vector4(2f * (xy - wz), 1f - 2f * (xx + zz), 2f * (yz + wx), 0f),
            new Vector4(2f * (xz + wy), 2f * (yz - wx), 1f - 2f * (xx + yy), 0f),
            new Vector4(0f, 0f, 0f, 1f));
    }

    public static Matrix4x4 Rotate(ref Quaternion q) => Rotate(q);

    public readonly Quaternion rotation => Quaternion.LookRotation(GetColumn(2), GetColumn(1));

    public readonly Vector3 lossyScale =>
        new(((Vector3)GetColumn(0)).magnitude, ((Vector3)GetColumn(1)).magnitude, ((Vector3)GetColumn(2)).magnitude);

    /// <summary>True when every element is within 0.000001 of the identity matrix.</summary>
    public readonly bool isIdentity
    {
        get
        {
            for (var i = 0; i < 16; i++)
            {
                var expected = i % 5 == 0 ? 1f : 0f;
                if (MathF.Abs(this[i] - expected) >= 0.000001f) return false;
            }
            return true;
        }
    }

    /// <summary>True when the matrix is a translation, rotation and non-zero scale: the bottom
    /// row is (0, 0, 0, 1), every element is finite, the upper 3x3 part is invertible and its
    /// columns are perpendicular to each other.</summary>
    public readonly bool ValidTRS()
    {
        var bottom = GetRow(3);
        if (MathF.Abs(bottom.x) > 1e-6f || MathF.Abs(bottom.y) > 1e-6f || MathF.Abs(bottom.z) > 1e-6f || MathF.Abs(bottom.w - 1f) > 1e-6f)
            return false;
        Vector3 c0 = GetColumn(0), c1 = GetColumn(1), c2 = GetColumn(2);
        var scale = lossyScale;
        if (!float.IsFinite(scale.x + scale.y + scale.z + m03 + m13 + m23)) return false;
        if (scale.x < 1e-9f || scale.y < 1e-9f || scale.z < 1e-9f) return false;
        return MathF.Abs(Vector3.Dot(c0, c1)) <= 1e-4f * scale.x * scale.y
            && MathF.Abs(Vector3.Dot(c0, c2)) <= 1e-4f * scale.x * scale.z
            && MathF.Abs(Vector3.Dot(c1, c2)) <= 1e-4f * scale.y * scale.z;
    }

    private static void Minors(Matrix4x4 m, Span<float> s, Span<float> c)
    {
        s[0] = m.m00 * m.m11 - m.m10 * m.m01;
        s[1] = m.m00 * m.m12 - m.m10 * m.m02;
        s[2] = m.m00 * m.m13 - m.m10 * m.m03;
        s[3] = m.m01 * m.m12 - m.m11 * m.m02;
        s[4] = m.m01 * m.m13 - m.m11 * m.m03;
        s[5] = m.m02 * m.m13 - m.m12 * m.m03;
        c[5] = m.m22 * m.m33 - m.m32 * m.m23;
        c[4] = m.m21 * m.m33 - m.m31 * m.m23;
        c[3] = m.m21 * m.m32 - m.m31 * m.m22;
        c[2] = m.m20 * m.m33 - m.m30 * m.m23;
        c[1] = m.m20 * m.m32 - m.m30 * m.m22;
        c[0] = m.m20 * m.m31 - m.m30 * m.m21;
    }

    private static float DeterminantFromMinors(ReadOnlySpan<float> s, ReadOnlySpan<float> c) =>
        s[0] * c[5] - s[1] * c[4] + s[2] * c[3] + s[3] * c[2] - s[4] * c[1] + s[5] * c[0];

    public static float Determinant(Matrix4x4 m)
    {
        Span<float> s = stackalloc float[6];
        Span<float> c = stackalloc float[6];
        Minors(m, s, c);
        return DeterminantFromMinors(s, c);
    }

    public static float Determinant(ref Matrix4x4 m) => Determinant(m);
    public readonly float determinant => Determinant(this);

    /// <summary>The inverse, or the zero matrix when the determinant is zero.</summary>
    public static Matrix4x4 Inverse(Matrix4x4 m)
    {
        Span<float> s = stackalloc float[6];
        Span<float> c = stackalloc float[6];
        Minors(m, s, c);
        var det = DeterminantFromMinors(s, c);
        if (det == 0f) return zero;
        var k = 1f / det;
        return new Matrix4x4(
            new Vector4(
                (m.m11 * c[5] - m.m12 * c[4] + m.m13 * c[3]) * k,
                (-m.m10 * c[5] + m.m12 * c[2] - m.m13 * c[1]) * k,
                (m.m10 * c[4] - m.m11 * c[2] + m.m13 * c[0]) * k,
                (-m.m10 * c[3] + m.m11 * c[1] - m.m12 * c[0]) * k),
            new Vector4(
                (-m.m01 * c[5] + m.m02 * c[4] - m.m03 * c[3]) * k,
                (m.m00 * c[5] - m.m02 * c[2] + m.m03 * c[1]) * k,
                (-m.m00 * c[4] + m.m01 * c[2] - m.m03 * c[0]) * k,
                (m.m00 * c[3] - m.m01 * c[1] + m.m02 * c[0]) * k),
            new Vector4(
                (m.m31 * s[5] - m.m32 * s[4] + m.m33 * s[3]) * k,
                (-m.m30 * s[5] + m.m32 * s[2] - m.m33 * s[1]) * k,
                (m.m30 * s[4] - m.m31 * s[2] + m.m33 * s[0]) * k,
                (-m.m30 * s[3] + m.m31 * s[1] - m.m32 * s[0]) * k),
            new Vector4(
                (-m.m21 * s[5] + m.m22 * s[4] - m.m23 * s[3]) * k,
                (m.m20 * s[5] - m.m22 * s[2] + m.m23 * s[1]) * k,
                (-m.m20 * s[4] + m.m21 * s[2] - m.m23 * s[0]) * k,
                (m.m20 * s[3] - m.m21 * s[1] + m.m22 * s[0]) * k));
    }

    public static Matrix4x4 Inverse(ref Matrix4x4 m) => Inverse(m);
    public readonly Matrix4x4 inverse => Inverse(this);

    /// <summary>Inverts a matrix whose bottom row is (0, 0, 0, 1). Returns false and leaves
    /// <paramref name="result"/> unchanged when the upper 3x3 part is singular.</summary>
    public static bool Inverse3DAffine(Matrix4x4 input, ref Matrix4x4 result)
    {
        var det = input.m00 * (input.m11 * input.m22 - input.m12 * input.m21)
            - input.m01 * (input.m10 * input.m22 - input.m12 * input.m20)
            + input.m02 * (input.m10 * input.m21 - input.m11 * input.m20);
        if (det == 0f) return false;
        var k = 1f / det;
        var column0 = new Vector4(
            (input.m11 * input.m22 - input.m12 * input.m21) * k,
            -(input.m10 * input.m22 - input.m12 * input.m20) * k,
            (input.m10 * input.m21 - input.m11 * input.m20) * k, 0f);
        var column1 = new Vector4(
            -(input.m01 * input.m22 - input.m02 * input.m21) * k,
            (input.m00 * input.m22 - input.m02 * input.m20) * k,
            -(input.m00 * input.m21 - input.m01 * input.m20) * k, 0f);
        var column2 = new Vector4(
            (input.m01 * input.m12 - input.m02 * input.m11) * k,
            -(input.m00 * input.m12 - input.m02 * input.m10) * k,
            (input.m00 * input.m11 - input.m01 * input.m10) * k, 0f);
        var linear = new Matrix4x4(column0, column1, column2, new Vector4(0f, 0f, 0f, 1f));
        var offset = linear.MultiplyVector(new Vector3(input.m03, input.m13, input.m23));
        linear.m03 = -offset.x;
        linear.m13 = -offset.y;
        linear.m23 = -offset.z;
        result = linear;
        return true;
    }

    public static bool Inverse3DAffine(ref Matrix4x4 input, ref Matrix4x4 result) => Inverse3DAffine(input, ref result);

    public static Matrix4x4 Transpose(Matrix4x4 m) => new(m.GetRow(0), m.GetRow(1), m.GetRow(2), m.GetRow(3));
    public static Matrix4x4 Transpose(ref Matrix4x4 m) => Transpose(m);
    public readonly Matrix4x4 transpose => Transpose(this);

    public static Matrix4x4 Ortho(float left, float right, float bottom, float top, float zNear, float zFar) => new(
        new Vector4(2f / (right - left), 0f, 0f, 0f),
        new Vector4(0f, 2f / (top - bottom), 0f, 0f),
        new Vector4(0f, 0f, -2f / (zFar - zNear), 0f),
        new Vector4(-(right + left) / (right - left), -(top + bottom) / (top - bottom), -(zFar + zNear) / (zFar - zNear), 1f));

    /// <summary>A perspective projection for a vertical field of view in degrees.</summary>
    public static Matrix4x4 Perspective(float fov, float aspect, float zNear, float zFar)
    {
        var f = 1f / MathF.Tan(fov * Mathf.Deg2Rad * 0.5f);
        return new Matrix4x4(
            new Vector4(f / aspect, 0f, 0f, 0f),
            new Vector4(0f, f, 0f, 0f),
            new Vector4(0f, 0f, -(zFar + zNear) / (zFar - zNear), -1f),
            new Vector4(0f, 0f, -2f * zFar * zNear / (zFar - zNear), 0f));
    }

    public static Matrix4x4 Frustum(float left, float right, float bottom, float top, float zNear, float zFar) => new(
        new Vector4(2f * zNear / (right - left), 0f, 0f, 0f),
        new Vector4(0f, 2f * zNear / (top - bottom), 0f, 0f),
        new Vector4((right + left) / (right - left), (top + bottom) / (top - bottom), -(zFar + zNear) / (zFar - zNear), -1f),
        new Vector4(0f, 0f, -2f * zFar * zNear / (zFar - zNear), 0f));

    public static Matrix4x4 Frustum(FrustumPlanes fp) => Frustum(fp.left, fp.right, fp.bottom, fp.top, fp.zNear, fp.zFar);
    public static Matrix4x4 Frustum(ref FrustumPlanes fp) => Frustum(fp);

    /// <summary>The planes of the projection this matrix represents, for a perspective
    /// matrix made by <see cref="Frustum(float, float, float, float, float, float)"/> or an
    /// orthographic one made by <see cref="Ortho"/>.</summary>
    public readonly FrustumPlanes decomposeProjection
    {
        get
        {
            if (m32 == 0f) return DecomposeOrthographic();
            var near = m23 / (m22 - 1f);
            return new FrustumPlanes
            {
                zNear = near,
                zFar = m23 / (m22 + 1f),
                left = near * (m02 - 1f) / m00,
                right = near * (m02 + 1f) / m00,
                bottom = near * (m12 - 1f) / m11,
                top = near * (m12 + 1f) / m11,
            };
        }
    }

    private readonly FrustumPlanes DecomposeOrthographic() => new()
    {
        zNear = (m23 + 1f) / m22,
        zFar = (m23 - 1f) / m22,
        left = (-m03 - 1f) / m00,
        right = (-m03 + 1f) / m00,
        bottom = (-m13 - 1f) / m11,
        top = (-m13 + 1f) / m11,
    };

    /// <summary>The placement of a viewer at <paramref name="from"/> looking at
    /// <paramref name="to"/>, <c>TRS(from, LookRotation(to - from, up), one)</c> as Unity
    /// documents it. Its inverse is the view matrix.</summary>
    public static Matrix4x4 LookAt(Vector3 from, Vector3 to, Vector3 up) =>
        TRS(from, Quaternion.LookRotation(to - from, up), Vector3.one);

    public static Matrix4x4 LookAt(ref Vector3 from, ref Vector3 to, ref Vector3 up) => LookAt(from, to, up);

    /// <summary>Transforms a point, including the translation, and divides by the resulting
    /// W so perspective projections work.</summary>
    public readonly Vector3 MultiplyPoint(Vector3 point)
    {
        var w = 1f / (m30 * point.x + m31 * point.y + m32 * point.z + m33);
        return new Vector3(
            (m00 * point.x + m01 * point.y + m02 * point.z + m03) * w,
            (m10 * point.x + m11 * point.y + m12 * point.z + m13) * w,
            (m20 * point.x + m21 * point.y + m22 * point.z + m23) * w);
    }

    public readonly Vector3 MultiplyPoint(ref Vector3 point) => MultiplyPoint(point);

    /// <summary>Transforms a point, including the translation, ignoring the bottom row.</summary>
    public readonly Vector3 MultiplyPoint3x4(Vector3 point) => new(
        m00 * point.x + m01 * point.y + m02 * point.z + m03,
        m10 * point.x + m11 * point.y + m12 * point.z + m13,
        m20 * point.x + m21 * point.y + m22 * point.z + m23);

    public readonly Vector3 MultiplyPoint3x4(ref Vector3 point) => MultiplyPoint3x4(point);

    /// <summary>Transforms a direction, ignoring the translation.</summary>
    public readonly Vector3 MultiplyVector(Vector3 vector) => new(
        m00 * vector.x + m01 * vector.y + m02 * vector.z,
        m10 * vector.x + m11 * vector.y + m12 * vector.z,
        m20 * vector.x + m21 * vector.y + m22 * vector.z);

    public readonly Vector3 MultiplyVector(ref Vector3 vector) => MultiplyVector(vector);

    /// <summary>Transforms the plane as a surface, using the inverse transpose of this matrix.</summary>
    public readonly Plane TransformPlane(Plane plane)
    {
        var equation = Transpose(Inverse(this)) * new Vector4(plane.normal.x, plane.normal.y, plane.normal.z, plane.distance);
        var length = ((Vector3)equation).magnitude;
        return new Plane(new Vector3(equation.x, equation.y, equation.z) / length, equation.w / length);
    }

    public readonly Plane TransformPlane(ref Plane plane) => TransformPlane(plane);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        format = string.IsNullOrEmpty(format) ? "F5" : format;
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        var text = new System.Text.StringBuilder();
        for (var row = 0; row < 4; row++)
        {
            for (var column = 0; column < 4; column++)
                text.Append(this[row, column].ToString(format, formatProvider)).Append(column < 3 ? '\t' : '\n');
        }
        return text.ToString();
    }
}
