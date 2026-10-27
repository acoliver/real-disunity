struct Parameters {
    size:u32,
    samples:u32,
    lut:u32,
    layers:u32,
    roughness:f32,
    remap:u32,
    visibility:u32,
    source_lod_tail:u32,
}
@group(0) @binding(0) var<uniform> params:Parameters;
@group(0) @binding(1) var environment:texture_cube<f32>;
@group(0) @binding(2) var environment_sampler:sampler;
@group(0) @binding(3) var output:texture_storage_2d_array<rgba16float,write>;
fn reverse_bits(value:u32)->u32 {
    var v=value;
    v=(v<<16u)|(v>>16u);
    v=((v&0x55555555u)<<1u)|((v&0xaaaaaaaau)>>1u);
    v=((v&0x33333333u)<<2u)|((v&0xccccccccu)>>2u);
    v=((v&0x0f0f0f0fu)<<4u)|((v&0xf0f0f0f0u)>>4u);
    return ((v&0x00ff00ffu)<<8u)|((v&0xff00ff00u)>>8u);
}
fn sample_half(index:u32,roughness:f32)->vec3<f32> {
    let xi=vec2(f32(index)/f32(params.samples),f32(reverse_bits(index))*2.3283064e-10);
    let alpha2=roughness*roughness*roughness*roughness;
    let cosine=sqrt((1.0-xi.y)/(1.0+(alpha2-1.0)*xi.y));
    let sine=sqrt(max(0.0,1.0-cosine*cosine));
    let phi=6.28318530718*xi.x;
    return vec3(cos(phi)*sine,sin(phi)*sine,cosine);
}
fn cube_ray(face:u32,uv:vec2<f32>)->vec3<f32> {
    var ray=vec3(-uv.x,-uv.y,-1.0);
    switch face {
        case 0u:{ray=vec3(1.0,-uv.y,-uv.x);}
        case 1u:{ray=vec3(-1.0,-uv.y,uv.x);}
        case 2u:{ray=vec3(uv.x,1.0,uv.y);}
        case 3u:{ray=vec3(uv.x,-1.0,-uv.y);}
        case 4u:{ray=vec3(uv.x,-uv.y,1.0);}
        default:{}
    }
    return normalize(ray);
}
fn brdf(no_v:f32,input_roughness:f32)->vec2<f32> {
    let roughness=rd_remap_roughness(input_roughness,params.remap);
    let v=vec3(sqrt(1.0-no_v*no_v),0.0,no_v);
    var sum=vec2(0.0);
    for(var i=0u;i<params.samples;i++) {
        let h=sample_half(i,roughness);
        let vo_h=max(dot(v,h),0.0);
        let l=2.0*vo_h*h-v;
        if l.z>0.0 {
            let visibility=rd_visibility(no_v,l.z,roughness,params.visibility*2u);
            let weight=4.0*visibility*l.z*vo_h/max(h.z,1e-7);
            let fc=pow(1.0-vo_h,5.0);
            sum+=vec2(1.0-fc,fc)*weight;
        }
    }
    return sum/f32(params.samples);
}
fn filtered(n:vec3<f32>)->vec3<f32> {
    let roughness=rd_remap_roughness(params.roughness,params.remap);
    var seed=vec3(0.0,0.0,1.0);
    if abs(n.z)>=0.999 { seed=vec3(1.0,0.0,0.0); }
    let tangent=normalize(cross(seed,n));
    let bitangent=cross(n,tangent);
    var sum=vec3(0.0);
    var weight=0.0;
    for(var i=0u;i<params.samples;i++) {
        let local=sample_half(i,roughness);
        let h=tangent*local.x+bitangent*local.y+n*local.z;
        let l=2.0*dot(n,h)*h-n;
        let no_l=max(dot(n,l),0.0);
        if no_l>0.0 {
            var mip=0.0;
            if roughness>0.0 {
                let alpha2=pow(roughness,4.0);
                let no_h=max(dot(n,h),0.0);
                let denominator=no_h*no_h*(alpha2-1.0)+1.0;
                // V=N, so the GGX reflection PDF D*NoH/(4*VoH) reduces to D/4.
                let pdf=alpha2/(12.56637061436*denominator*denominator);
                let resolution=f32(textureDimensions(environment).x);
                let texel_solid_angle=12.56637061436/(6.0*resolution*resolution);
                let sample_solid_angle=1.0/(f32(params.samples)*pdf);
                mip=clamp(0.5*log2(sample_solid_angle/texel_solid_angle),0.0,f32(textureNumLevels(environment)-min(params.source_lod_tail+1u,textureNumLevels(environment))));
            }
            sum+=textureSampleLevel(environment,environment_sampler,l,mip).rgb*no_l;
            weight+=no_l;
        }
    }
    return sum/weight;
}
@compute @workgroup_size(8,8,1)
fn integrate(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=params.size || id.y>=params.size || id.z>=params.layers { return; }
    let uv=(vec2<f32>(id.xy)+0.5)/f32(params.size);
    var value=vec4(0.0,0.0,0.0,1.0);
    if params.lut!=0u {
        value=vec4(brdf(uv.x,uv.y),0.0,1.0);
    } else {
        value=vec4(filtered(cube_ray(id.z,uv*2.0-1.0)),1.0);
    }
    textureStore(output,vec2<i32>(id.xy),i32(id.z),value);
}
