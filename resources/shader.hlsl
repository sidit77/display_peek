struct VSInput {
    float3 position: POSITION;
    float2 uv: TEXCOORD;
};

struct VSOutput {
    float4 position: SV_POSITION;
    float2 uv: TEXCOORD0;
};

cbuffer cbPerObject
{
    float4x4 transform;
};

Texture2D tex: register(t0);
SamplerState samp: register(s0);


VSOutput vs_main(VSInput input) {
    VSOutput output;
    output.position = mul(float4(input.position, 1.0f), transform);
    output.uv= input.uv;
    return output;
}

float4 ps_main(VSOutput vs): SV_TARGET {
    return tex.Sample(samp, vs.uv);
}