// CMAA2 (Conservative Morphological Anti-Aliasing 2.3), single-sample path.
//
// THIS FILE WAS MODIFIED: it is a port of CMAA2.hlsl to WGSL by the Real Disunity project.
// The structure, constants and heuristics follow the original; see crates/render-post/README.md
// for the list of deviations.
//
// Original work: Conservative Morphological Anti-Aliasing, version 2.3
// Author: Filip Strugar (filip.strugar@intel.com)
// Source: https://github.com/GameTechDev/CMAA2 (Projects/CMAA2/CMAA2/CMAA2.hlsl)
//
// Copyright (c) 2018, Intel Corporation
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// The full license text is in crates/render-post/LICENSE-APACHE-2.0-CMAA2.txt.

override THRESHOLD: f32 = 0.07;
override EXTRA_SHARPNESS: bool = false;

@group(0) @binding(2) var work_in: texture_2d<f32>;

@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}

@fragment fn cmaa2_present(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(work_in, vec2<i32>(p.xy), 0);
}
