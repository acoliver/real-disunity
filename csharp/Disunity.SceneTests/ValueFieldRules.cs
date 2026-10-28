using Disunity.Hosting;
using UnityEngine;
using static Check;

public sealed class Values : MonoBehaviour
{
    public Vector4 v4;
    public Quaternion rotation;
    public Vector2Int cell;
    public Vector3Int voxel;
    public Rect rect;
    public RectInt area;
    public Color32 packed;
    public Bounds bounds;
    public BoundsInt cells;
    public Matrix4x4 matrix;
    public AnimationCurve? curve;
    public AnimationCurve? absentCurve = new();
    public Gradient? gradient;
    public Gradient? absentGradient = new();
    public List<Vector4> several = new();
}

/// <summary>Unity's value types bind from the plan JSON the project loader and the player decoder both
/// produce: every builtin the plan can name, with integers parsed exactly and unknown curve modes refused.</summary>
internal static class ValueFieldRules
{
    private const string MatrixJson = """
        {"m00": "0", "m10": "10", "m20": "20", "m30": "30", "m01": "1", "m11": "11", "m21": "21", "m31": "31",
         "m02": "2", "m12": "12", "m22": "22", "m32": "32", "m03": "3", "m13": "13", "m23": "23", "m33": "33"}
        """;

    private const string CurveJson = """
        {"m_Curve": [
           {"time": "0", "value": "1", "inSlope": "0", "outSlope": "2", "weightedMode": "2", "inWeight": "0.25", "outWeight": "0.5"},
           {"time": "-0.5", "value": "3", "inSlope": "-Infinity", "outSlope": "Infinity", "weightedMode": "0", "inWeight": "0.3333333432674408", "outWeight": "0.3333333432674408"}],
         "m_PreInfinity": "2", "m_PostInfinity": "2", "m_RotationOrder": "4"}
        """;

    private const string GradientJson = """
        {"key0": {"r": "1", "g": "0", "b": "0", "a": "1"}, "key1": {"r": "0", "g": "0", "b": "1", "a": "0.5"},
         "key2": {"r": "0", "g": "1", "b": "0", "a": "0"}, "key3": {"r": "0", "g": "0", "b": "0", "a": "0"},
         "key4": {"r": "0", "g": "0", "b": "0", "a": "0"}, "key5": {"r": "0", "g": "0", "b": "0", "a": "0"},
         "key6": {"r": "0", "g": "0", "b": "0", "a": "0"}, "key7": {"r": "0", "g": "0", "b": "0", "a": "0"},
         "ctime0": "0", "ctime1": "32768", "ctime2": "65535", "ctime3": "0", "ctime4": "0", "ctime5": "0", "ctime6": "0", "ctime7": "0",
         "atime0": "0", "atime1": "65535", "atime2": "0", "atime3": "0", "atime4": "0", "atime5": "0", "atime6": "0", "atime7": "0",
         "m_Mode": "1", "m_ColorSpace": "1", "m_NumColorKeys": "3", "m_NumAlphaKeys": "2"}
        """;

    private static readonly string Fields = """
        {"v4": {"x": "1", "y": "-2.5", "z": "3", "w": "4"}, "rotation": {"x": "0", "y": "0.5", "z": "0", "w": "0.5"},
         "cell": {"x": "16777217", "y": "-3"}, "voxel": {"x": "-16777217", "y": "0", "z": "7"},
         "rect": {"x": "1", "y": "2", "width": "3", "height": "4"}, "area": {"x": "-5", "y": "6", "width": "16777217", "height": "8"},
         "packed": {"rgba": "67305985"},
         "bounds": {"m_Center": {"x": "1", "y": "2", "z": "3"}, "m_Extent": {"x": "0.5", "y": "1", "z": "1.5"}},
         "cells": {"m_Position": {"x": "1", "y": "-2", "z": "3"}, "m_Size": {"x": "4", "y": "5", "z": "6"}},
         "matrix": MATRIX, "curve": CURVE, "absentCurve": null, "gradient": GRADIENT, "absentGradient": null,
         "several": [{"x": "1", "y": "2", "z": "3", "w": "4"}, {"x": "5", "y": "6", "z": "7", "w": "8"}]}
        """.Replace("MATRIX", MatrixJson).Replace("CURVE", CurveJson).Replace("GRADIENT", GradientJson);

    /// <summary>Runs inside <see cref="LoaderRules.Run"/>'s world, under its PASS line.</summary>
    public static void Check()
    {
        ValuesBindFromTheirLayouts();
        CurvesAndGradientsBind();
        BadValuesFailWithTheFieldNamed();
    }

    private static Values Load(string fields, out IDisposable scene)
    {
        var loaded = DisunityHost.LoadScene(LoaderRules.Plan(fields, script: nameof(Values)), LoaderRules.Options);
        scene = loaded;
        return loaded.Objects[0].GetComponent<Values>()!;
    }

    private static void ValuesBindFromTheirLayouts()
    {
        var values = Load(Fields, out var scene);
        using (scene)
        {
            Require(values.v4 == new Vector4(1, -2.5f, 3, 4), "Vector4 binds x, y, z, w");
            Require(values.rotation.Equals(new Quaternion(0, 0.5f, 0, 0.5f)), "Quaternion binds x, y, z, w");
            Require(values.cell == new Vector2Int(16777217, -3) && values.voxel == new Vector3Int(-16777217, 0, 7),
                $"integer vectors parse exactly, not through float: {values.cell} {values.voxel}");
            Require(values.rect == new Rect(1, 2, 3, 4), "Rect binds x, y, width, height");
            Require(values.area.x == -5 && values.area.y == 6 && values.area.width == 16777217 && values.area.height == 8,
                "RectInt parses its integers exactly");
            Require(values.packed.r == 1 && values.packed.g == 2 && values.packed.b == 3 && values.packed.a == 4,
                $"Color32 unpacks rgba little-endian, red in the lowest byte: {values.packed}");
            Require(values.bounds.center == new Vector3(1, 2, 3) && values.bounds.extents == new Vector3(0.5f, 1, 1.5f)
                && values.bounds.size == new Vector3(1, 2, 3), "Bounds binds m_Extent as extents, so its size is twice that");
            Require(values.cells.position == new Vector3Int(1, -2, 3) && values.cells.size == new Vector3Int(4, 5, 6),
                "BoundsInt binds m_Position and m_Size");
            for (var row = 0; row < 4; row++)
                for (var column = 0; column < 4; column++)
                    Require(values.matrix[row, column] == row * 10 + column, $"Matrix4x4 m{row}{column} is row {row}, column {column}");
            Require(values.several.Count == 2 && values.several[1] == new Vector4(5, 6, 7, 8), "a list of Vector4 binds");
            Require(values.absentCurve == null && values.absentGradient == null, "a null AnimationCurve or Gradient is null");
        }
    }

    private static void CurvesAndGradientsBind()
    {
        var values = Load(Fields, out var scene);
        using (scene)
        {
            var curve = values.curve!;
            Require(curve.length == 2 && curve[0].time == -0.5f && curve[1].time == 0f, "curve keys bind and stay sorted by time");
            var (first, second) = (curve[0], curve[1]);
            Require(float.IsNegativeInfinity(first.inTangent) && float.IsPositiveInfinity(first.outTangent) && first.value == 3f,
                "infinite slopes bind");
            Require(second.value == 1f && second.inTangent == 0f && second.outTangent == 2f && second.inWeight == 0.25f && second.outWeight == 0.5f,
                "slopes and weights bind as in and out tangents and weights");
            Require(second.weightedMode == WeightedMode.Out && first.weightedMode == WeightedMode.None, "weighted modes bind");
            Require(curve.preWrapMode == WrapMode.ClampForever && curve.postWrapMode == WrapMode.ClampForever,
                "stored infinity mode 2 is ClampForever, which holds the end values");
            Require(curve.Evaluate(-5f) == 3f && curve.Evaluate(5f) == 1f, "the bound curve evaluates with those wrap modes");
            var gradient = values.gradient!;
            Require(gradient.colorKeyCount == 3 && gradient.alphaKeyCount == 2, "the key counts say how many keys are in use");
            Require(gradient.mode == GradientMode.Fixed && gradient.colorSpace == ColorSpace.Linear, "mode and color space bind");
            var colors = gradient.colorKeys;
            Require(colors[0].color == new Color(1, 0, 0, 1) && colors[1].color == new Color(0, 0, 1, 1) && colors[2].color == new Color(0, 1, 0, 1),
                "color keys take their rgb from keyN with alpha 1");
            Require(colors[0].time == 0f && colors[1].time == 32768f / 65535f && colors[2].time == 1f, "color key times scale by 65535");
            var alphas = gradient.alphaKeys;
            Require(alphas[0].alpha == 1f && alphas[1].alpha == 0.5f && alphas[0].time == 0f && alphas[1].time == 1f,
                "alpha keys take their alpha from keyN.a and their time from atimeN");
            Require(gradient.Evaluate(0.2f) == new Color(0, 0, 1, 0.5f), "the bound Fixed gradient reads the next key");
        }
    }

    private static void BadValuesFailWithTheFieldNamed()
    {
        string With(string find, string replace) => LoaderRules.Plan(Fields.Replace(find, replace), script: nameof(Values));
        LoaderRules.Fails(With("\"m_PostInfinity\": \"2\"", "\"m_PostInfinity\": \"4\""), "Values.curve: cannot read");
        LoaderRules.Fails(With("\"m_PostInfinity\": \"2\"", "\"m_PostInfinity\": \"4\""), "curve infinity mode 4");
        LoaderRules.Fails(With("\"x\": \"16777217\", \"y\": \"-3\"", "\"x\": \"1.5\", \"y\": \"-3\""), "Values.cell: cannot read");
        LoaderRules.Fails(With("\"m_NumColorKeys\": \"3\"", "\"m_NumColorKeys\": \"9\""), "at most 8 keys");
        LoaderRules.Fails(With("\"rgba\": \"67305985\"", "\"rgba\": \"-1\""), "Values.packed: cannot read");
    }
}
