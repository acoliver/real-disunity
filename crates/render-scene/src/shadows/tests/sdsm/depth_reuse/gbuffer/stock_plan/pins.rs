//! The pinned plans, taken from the frame before its plan was built in one place.
use crate::compositor::plan_trace::Pin;

pub(super) const SCREEN_UNFITTED: &[Pin] = &[Pin {
    prefix: false,
    passes: &[
        "ShadowCasters(0)#0  / shadow depth:C",
        "DepthNormalPrepass#0 prepass normals:C,reflection material:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
        "DepthPyramid(0)#0 linear camera depth:C / ",
        "DepthPyramid(1)#0 linear camera depth@v1:C / ",
        "DepthPyramid(2)#0 linear camera depth@v2:C / ",
        "DepthPyramid(3)#0 linear camera depth@v3:C / ",
        "DepthPyramid(4)#0 linear camera depth@v4:C / ",
        "DepthPyramid(5)#0 linear camera depth@v5:C / ",
        "DepthPyramid(6)#0 linear camera depth@v6:C / ",
        "DepthPyramid(7)#0 linear camera depth@v7:C / ",
        "DepthPyramid(8)#0 linear camera depth@v8:C / ",
        "DepthPyramid(9)#0 linear camera depth@v9:C / ",
        "Ssao#0 raw ambient occlusion:C / ",
        "SsaoBlur#0 ambient occlusion:C / ",
        "Opaque#0 scene color:C,forward environment specular:C,forward reflection factor:C / scene depth:L!ro",
        "AfterOpaqueSnapshot#0 opaque snapshot color:C / ",
        "AfterOpaqueDepth#0 opaque snapshot depth:C / ",
        "TransparentWater#0 scene color:L,water receiver normals:C,water receiver material:C,water environment specular:C / scene depth:L",
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

pub(super) const SCREEN_SAME_FRAME: &[Pin] = &[
    Pin {
        prefix: true,
        passes: &[
            "DepthNormalPrepass#0 prepass normals:C,reflection material:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
        ],
        effects: &[],
        operations: &[],
    },
    Pin {
        prefix: false,
        passes: &[
            "ShadowCasters(0)#0  / shadow depth:C",
            "DepthNormalPrepass#0 prepass normals:C,reflection material:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
            "DepthPyramid(1)#0 linear camera depth@v1:C / ",
            "DepthPyramid(2)#0 linear camera depth@v2:C / ",
            "DepthPyramid(3)#0 linear camera depth@v3:C / ",
            "DepthPyramid(4)#0 linear camera depth@v4:C / ",
            "DepthPyramid(5)#0 linear camera depth@v5:C / ",
            "DepthPyramid(6)#0 linear camera depth@v6:C / ",
            "DepthPyramid(7)#0 linear camera depth@v7:C / ",
            "DepthPyramid(8)#0 linear camera depth@v8:C / ",
            "DepthPyramid(9)#0 linear camera depth@v9:C / ",
            "Ssao#0 raw ambient occlusion:C / ",
            "SsaoBlur#0 ambient occlusion:C / ",
            "Opaque#0 scene color:C,forward environment specular:C,forward reflection factor:C / scene depth:L!ro",
            "AfterOpaqueSnapshot#0 opaque snapshot color:C / ",
            "AfterOpaqueDepth#0 opaque snapshot depth:C / ",
            "TransparentWater#0 scene color:L,water receiver normals:C,water receiver material:C,water environment specular:C / scene depth:L",
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
    },
];

pub(super) const SCREEN_INTERACTIVE: &[Pin] = &[
    Pin {
        prefix: true,
        passes: &[
            "DepthNormalPrepass#0 prepass normals:C,reflection material:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
        ],
        effects: &[],
        operations: &[],
    },
    Pin {
        prefix: false,
        passes: &[
            "ShadowCasters(0)#0  / shadow depth:C",
            "DepthNormalPrepass#0 prepass normals:C,reflection material:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
            "DepthPyramid(1)#0 linear camera depth@v1:C / ",
            "DepthPyramid(2)#0 linear camera depth@v2:C / ",
            "DepthPyramid(3)#0 linear camera depth@v3:C / ",
            "DepthPyramid(4)#0 linear camera depth@v4:C / ",
            "DepthPyramid(5)#0 linear camera depth@v5:C / ",
            "DepthPyramid(6)#0 linear camera depth@v6:C / ",
            "DepthPyramid(7)#0 linear camera depth@v7:C / ",
            "DepthPyramid(8)#0 linear camera depth@v8:C / ",
            "DepthPyramid(9)#0 linear camera depth@v9:C / ",
            "Ssao#0 raw ambient occlusion:C / ",
            "SsaoBlur#0 ambient occlusion:C / ",
            "Opaque#0 scene color:C,forward environment specular:C,forward reflection factor:C / scene depth:L!ro",
            "AfterOpaqueSnapshot#0 opaque snapshot color:C / ",
            "AfterOpaqueDepth#0 opaque snapshot depth:C / ",
            "TransparentWater#0 scene color:L,water receiver normals:C,water receiver material:C,water environment specular:C / scene depth:L",
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
    },
];

// The frames below fit shadow depth with no SSAO or reflections. Nothing samples computed
// occlusion in them, so each runs the prepass and level 0 of the depth hierarchy only and
// leaves out the occlusion kernel, its filter and the coarser levels.
pub(super) const FORWARD_UNFITTED: &[Pin] = &[Pin {
    prefix: false,
    passes: &[
        "ShadowCasters(0)#0  / shadow depth:C",
        "DepthNormalPrepass#0 prepass normals:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
        "DepthPyramid(0)#0 linear camera depth:C / ",
        "Opaque#0 scene color:C / scene depth:L!ro",
    ],
    effects: &[],
    operations: &[],
}];

pub(super) const FORWARD_SAME_FRAME: &[Pin] = &[
    Pin {
        prefix: true,
        passes: &[
            "DepthNormalPrepass#0 prepass normals:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
        ],
        effects: &[],
        operations: &[],
    },
    Pin {
        prefix: false,
        passes: &[
            "ShadowCasters(0)#0  / shadow depth:C",
            "DepthNormalPrepass#0 prepass normals:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
            "Opaque#0 scene color:C / scene depth:L!ro",
        ],
        effects: &[],
        operations: &[],
    },
];

pub(super) const FORWARD_INTERACTIVE: &[Pin] = &[
    Pin {
        prefix: true,
        passes: &[
            "DepthNormalPrepass#0 prepass normals:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
        ],
        effects: &[],
        operations: &[],
    },
    Pin {
        prefix: false,
        passes: &[
            "ShadowCasters(0)#0  / shadow depth:C",
            "DepthNormalPrepass#0 prepass normals:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
            "Opaque#0 scene color:C / scene depth:L!ro",
        ],
        effects: &[],
        operations: &[],
    },
];

pub(super) const OBSERVATION_UNFITTED: &[Pin] = &[Pin {
    prefix: false,
    passes: &[
        "ShadowCasters(0)#0  / shadow depth:C",
        "DepthNormalPrepass#0 prepass normals:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
        "DepthPyramid(0)#0 linear camera depth:C / ",
        "OpaqueGbuffer#0 gbuffer g0:C,gbuffer g1:C,gbuffer g2:C,gbuffer g3:C / scene depth:L!ro",
        "OpaqueLighting#0 deferred lighting colour:C / ",
        "Opaque#0 scene color:C / scene depth:L!ro",
    ],
    effects: &[],
    operations: &[],
}];

pub(super) const OBSERVATION_SAME_FRAME: &[Pin] = &[
    Pin {
        prefix: true,
        passes: &[
            "DepthNormalPrepass#0 prepass normals:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
        ],
        effects: &[],
        operations: &[],
    },
    Pin {
        prefix: false,
        passes: &[
            "ShadowCasters(0)#0  / shadow depth:C",
            "DepthNormalPrepass#0 prepass normals:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
            "OpaqueGbuffer#0 gbuffer g0:C,gbuffer g1:C,gbuffer g2:C,gbuffer g3:C / scene depth:L!ro",
            "OpaqueLighting#0 deferred lighting colour:C / ",
            "Opaque#0 scene color:C / scene depth:L!ro",
        ],
        effects: &[],
        operations: &[],
    },
];

pub(super) const OBSERVATION_INTERACTIVE: &[Pin] = &[
    Pin {
        prefix: true,
        passes: &[
            "DepthNormalPrepass#0 prepass normals:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
        ],
        effects: &[],
        operations: &[],
    },
    Pin {
        prefix: false,
        passes: &[
            "ShadowCasters(0)#0  / shadow depth:C",
            "DepthNormalPrepass#0 prepass normals:C,receiver displacement xyzw:C,receiver displacement yz:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
            "OpaqueGbuffer#0 gbuffer g0:C,gbuffer g1:C,gbuffer g2:C,gbuffer g3:C / scene depth:L!ro",
            "OpaqueLighting#0 deferred lighting colour:C / ",
            "Opaque#0 scene color:C / scene depth:L!ro",
        ],
        effects: &[],
        operations: &[],
    },
];

pub(super) const RENDER_UNFITTED: &[Pin] = &[Pin {
    prefix: false,
    passes: &[
        "ShadowCasters(0)#0  / shadow depth:C",
        "DepthNormalPrepass#0 gbuffer g0:C,gbuffer g1:C,gbuffer g2:C,gbuffer g3:C,prepass normals:C / scene depth:C",
        "DepthPyramid(0)#0 linear camera depth:C / ",
        "OpaqueLighting#0 scene color:C / ",
        "Opaque#0 scene color:L / scene depth:L",
    ],
    effects: &[],
    operations: &[],
}];

pub(super) const RENDER_SAME_FRAME: &[Pin] = &[
    Pin {
        prefix: true,
        passes: &[
            "DepthNormalPrepass#0 gbuffer g0:C,gbuffer g1:C,gbuffer g2:C,gbuffer g3:C,prepass normals:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
        ],
        effects: &[],
        operations: &[],
    },
    Pin {
        prefix: false,
        passes: &[
            "ShadowCasters(0)#0  / shadow depth:C",
            "DepthNormalPrepass#0 gbuffer g0:C,gbuffer g1:C,gbuffer g2:C,gbuffer g3:C,prepass normals:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
            "OpaqueLighting#0 scene color:C / ",
            "Opaque#0 scene color:L / scene depth:L",
        ],
        effects: &[],
        operations: &[],
    },
];

pub(super) const RENDER_INTERACTIVE: &[Pin] = &[
    Pin {
        prefix: true,
        passes: &[
            "DepthNormalPrepass#0 gbuffer g0:C,gbuffer g1:C,gbuffer g2:C,gbuffer g3:C,prepass normals:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
        ],
        effects: &[],
        operations: &[],
    },
    Pin {
        prefix: false,
        passes: &[
            "ShadowCasters(0)#0  / shadow depth:C",
            "DepthNormalPrepass#0 gbuffer g0:C,gbuffer g1:C,gbuffer g2:C,gbuffer g3:C,prepass normals:C / scene depth:C",
            "DepthPyramid(0)#0 linear camera depth:C / ",
            "OpaqueLighting#0 scene color:C / ",
            "Opaque#0 scene color:L / scene depth:L",
        ],
        effects: &[],
        operations: &[],
    },
];
