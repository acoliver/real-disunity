namespace UnityEngine;

public struct Vector2 : IEquatable<Vector2>
{
    public float x, y;
    public Vector2(float x, float y) { this.x = x; this.y = y; }
    public static Vector2 zero => new(0, 0);
    public static Vector2 one => new(1, 1);
    public float magnitude => MathF.Sqrt(sqrMagnitude);
    public float sqrMagnitude => x * x + y * y;
    public Vector2 normalized => magnitude > 1e-20f ? this / magnitude : zero;
    public void Normalize() => this = normalized;
    public static Vector2 operator +(Vector2 a, Vector2 b) => new(a.x + b.x, a.y + b.y);
    public static Vector2 operator -(Vector2 a, Vector2 b) => new(a.x - b.x, a.y - b.y);
    public static Vector2 operator -(Vector2 a) => new(-a.x, -a.y);
    public static Vector2 operator *(Vector2 a, float b) => new(a.x * b, a.y * b);
    public static Vector2 operator *(float b, Vector2 a) => a * b;
    public static Vector2 operator /(Vector2 a, float b) => new(a.x / b, a.y / b);
    public static bool operator ==(Vector2 a, Vector2 b) => a.Equals(b);
    public static bool operator !=(Vector2 a, Vector2 b) => !a.Equals(b);
    public static float Dot(Vector2 a, Vector2 b) => a.x * b.x + a.y * b.y;
    public static float Distance(Vector2 a, Vector2 b) => (a - b).magnitude;
    public bool Equals(Vector2 other) => x.Equals(other.x) && y.Equals(other.y);
    public override bool Equals(object? obj) => obj is Vector2 v && Equals(v);
    public override int GetHashCode() => HashCode.Combine(x, y);
    public override string ToString() => $"({x}, {y})";
    public static implicit operator Vector3(Vector2 v) => new(v.x, v.y, 0);
}

public struct Vector3 : IEquatable<Vector3>
{
    public float x, y, z;
    public Vector3(float x, float y, float z = 0) { this.x = x; this.y = y; this.z = z; }
    public static Vector3 zero => new(0, 0, 0);
    public static Vector3 one => new(1, 1, 1);
    public static Vector3 up => new(0, 1, 0);
    public static Vector3 right => new(1, 0, 0);
    public float magnitude => MathF.Sqrt(x*x+y*y+z*z);
    public Vector3 normalized => magnitude > 1e-20f ? this / magnitude : zero;
    public static Vector3 operator +(Vector3 a, Vector3 b) => new(a.x+b.x,a.y+b.y,a.z+b.z);
    public static Vector3 operator -(Vector3 a, Vector3 b) => new(a.x-b.x,a.y-b.y,a.z-b.z);
    public static Vector3 operator *(Vector3 a,float b) => new(a.x*b,a.y*b,a.z*b);
    public static Vector3 operator /(Vector3 a,float b) => new(a.x/b,a.y/b,a.z/b);
    public static bool operator ==(Vector3 a, Vector3 b) => a.Equals(b);
    public static bool operator !=(Vector3 a, Vector3 b) => !a.Equals(b);
    public bool Equals(Vector3 v) => x.Equals(v.x)&&y.Equals(v.y)&&z.Equals(v.z);
    public override bool Equals(object? o) => o is Vector3 v&&Equals(v);
    public override int GetHashCode()=>HashCode.Combine(x,y,z);
    public override string ToString()=>$"({x}, {y}, {z})";
    public static implicit operator Vector2(Vector3 v)=>new(v.x,v.y);
}

public struct Vector2Int : IEquatable<Vector2Int>
{
    public int x, y;
    public Vector2Int(int x,int y){this.x=x;this.y=y;}
    public static Vector2Int zero => new(0,0);
    public static Vector2Int up => new(0,1);
    public static Vector2Int down => new(0,-1);
    public static Vector2Int left => new(-1,0);
    public static Vector2Int right => new(1,0);
    public static Vector2Int operator +(Vector2Int a,Vector2Int b)=>new(a.x+b.x,a.y+b.y);
    public static Vector2Int operator -(Vector2Int a,Vector2Int b)=>new(a.x-b.x,a.y-b.y);
    public static Vector2Int operator /(Vector2Int value, int divisor)=>new(value.x/divisor,value.y/divisor);
    public static bool operator ==(Vector2Int a,Vector2Int b)=>a.Equals(b);
    public static bool operator !=(Vector2Int a,Vector2Int b)=>!a.Equals(b);
    public bool Equals(Vector2Int v)=>x==v.x&&y==v.y;
    public override bool Equals(object? o)=>o is Vector2Int v&&Equals(v);
    public override int GetHashCode()=>HashCode.Combine(x,y);
    public override string ToString()=>$"({x}, {y})";
}

public struct Vector3Int : IEquatable<Vector3Int>
{
    public int x,y,z;
    public Vector3Int(int x,int y,int z=0){this.x=x;this.y=y;this.z=z;}
    public bool Equals(Vector3Int v)=>x==v.x&&y==v.y&&z==v.z;
    public override bool Equals(object? o)=>o is Vector3Int v&&Equals(v);
    public override int GetHashCode()=>HashCode.Combine(x,y,z);
}

public struct Quaternion
{
    public float x,y,z,w;
    public Quaternion(float x,float y,float z,float w){this.x=x;this.y=y;this.z=z;this.w=w;}
    public static Quaternion identity => new(0,0,0,1);
    public static Quaternion Euler(float x,float y,float z)
    {
        const float d=MathF.PI/360f; var hx=x*d;var hy=y*d;var hz=z*d;
        var sx=MathF.Sin(hx);var cx=MathF.Cos(hx);var sy=MathF.Sin(hy);var cy=MathF.Cos(hy);var sz=MathF.Sin(hz);var cz=MathF.Cos(hz);
        return new(sx*cy*cz-cx*sy*sz,cx*sy*cz+sx*cy*sz,cx*cy*sz-sx*sy*cz,cx*cy*cz+sx*sy*sz);
    }
}

public struct Color
{
    public float r,g,b,a;
    public Color(float r,float g,float b,float a=1){this.r=r;this.g=g;this.b=b;this.a=a;}
    public static Color white=>new(1,1,1,1); public static Color black=>new(0,0,0,1);
}
public struct Rect { public float x,y,width,height; public Rect(float x,float y,float width,float height){this.x=x;this.y=y;this.width=width;this.height=height;} }

public static class Mathf
{
    public static float Min(float a, float b) => MathF.Min(a, b);
    public static float Max(float a, float b) => MathF.Max(a, b);
    public static float Abs(float value) => MathF.Abs(value);
    public static int Abs(int value) => Math.Abs(value);
    public static float Clamp(float value, float min, float max) => Math.Clamp(value, min, max);
    public static float Sqrt(float value) => MathF.Sqrt(value);
}
