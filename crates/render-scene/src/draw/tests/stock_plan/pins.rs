//! The plans an unshadowed scene frame submits, pinned before the plan was built in one place.
use crate::compositor::plan_trace::Pin;

pub(super) const FORWARD: &[Pin] = &[Pin {
    prefix: false,
    passes: &["Opaque#0 scene color:C / scene depth:C"],
    effects: &[],
    operations: &[],
}];

pub(super) const REFLECTIONS: &[Pin] = &[Pin {
    prefix: false,
    passes: &[
        "DepthNormalPrepass#0 prepass normals:C,reflection material:C / scene depth:C",
        "DepthPyramid(0)#0 linear camera depth:C / ",
        "DepthPyramid(1)#0 linear camera depth@v1:C / ",
        "DepthPyramid(2)#0 linear camera depth@v2:C / ",
        "DepthPyramid(3)#0 linear camera depth@v3:C / ",
        "DepthPyramid(4)#0 linear camera depth@v4:C / ",
        "DepthPyramid(5)#0 linear camera depth@v5:C / ",
        "DepthPyramid(6)#0 linear camera depth@v6:C / ",
        "DepthPyramid(7)#0 linear camera depth@v7:C / ",
        "Ssao#0 raw ambient occlusion:C / ",
        "SsaoBlur#0 ambient occlusion:C / ",
        "Opaque#0 scene color:C,forward environment specular:C,forward reflection factor:C / scene depth:L",
        "AfterOpaqueSnapshot#0 opaque snapshot color:C / ",
        "AfterOpaqueDepth#0 opaque snapshot depth:C / ",
        "AfterWaterDepth#0 post-water depth:C / ",
        "ReflectionDepthPyramid(0)#0 final reflection depth:C / ",
        "ReflectionDepthPyramid(1)#0 final reflection depth@v1:C / ",
        "ReflectionDepthPyramid(2)#0 final reflection depth@v2:C / ",
        "ReflectionDepthPyramid(3)#0 final reflection depth@v3:C / ",
        "ReflectionDepthPyramid(4)#0 final reflection depth@v4:C / ",
        "ReflectionDepthPyramid(5)#0 final reflection depth@v5:C / ",
        "ReflectionDepthPyramid(6)#0 final reflection depth@v6:C / ",
        "ReflectionTrace#0 reflection radiance:C / ",
        "ReflectionMip(1)#0 reflection radiance@v1:C / ",
        "ReflectionMip(2)#0 reflection radiance@v2:C / ",
        "ReflectionMip(3)#0 reflection radiance@v3:C / ",
        "ReflectionMip(4)#0 reflection radiance@v4:C / ",
        "ReflectionMip(5)#0 reflection radiance@v5:C / ",
        "ScreenSpaceAfterTransparent#0 scene color:L / ",
    ],
    effects: &["SSAO", "SSR"],
    operations: &[],
}];
