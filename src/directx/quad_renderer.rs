use std::mem::size_of;
use windows::Win32::Graphics::Direct3D11::{D3D11_APPEND_ALIGNED_ELEMENT, D3D11_BIND_INDEX_BUFFER, D3D11_BIND_VERTEX_BUFFER, D3D11_BUFFER_DESC, D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA, D3D11_SUBRESOURCE_DATA, D3D11_USAGE_DEFAULT, ID3D11Buffer, ID3D11InputLayout, ID3D11PixelShader, ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11VertexShader};
use anyhow::Result;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R32_UINT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32G32B32_FLOAT};
use crate::directx::Direct3D;

#[repr(C)]
struct Vertex {
    position: [f32; 3],
    tex: [f32; 2],
}

impl Vertex {
    const fn new(position: [f32; 3], tex: [f32; 2]) -> Self {
        Self { position, tex }
    }
}

const VERTICES: [Vertex; 4] = [
    Vertex::new([-1.0, 1.0, 0.0], [0.0, 0.0]),
    Vertex::new([1.0, 1.0, 0.0], [1.0, 0.0]),
    Vertex::new([-1.0, -1.0, 0.0], [0.0, 1.0]),
    Vertex::new([1.0, -1.0, 0.0], [1.0, 1.0]),
];

const INDICES: [u32; 6] = [0, 1, 2, 1, 3, 2];

pub struct QuadRenderer {
    vertex_buffer: ID3D11Buffer,
    index_buffer: ID3D11Buffer,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    input_layout: ID3D11InputLayout
}

impl QuadRenderer {

    pub fn new(d3d: &Direct3D) -> Result<Self>{
        let vertex_buffer = unsafe {
            d3d.device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: size_of::<[Vertex; 4]>() as _,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_VERTEX_BUFFER,
                    ..Default::default()
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: VERTICES.as_ptr() as _,
                    ..Default::default()
                }),
            )?
        };
        let index_buffer = unsafe {
            d3d.device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: size_of::<[u32; 6]>() as _,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_INDEX_BUFFER,
                    ..Default::default()
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: INDICES.as_ptr() as _,
                    ..Default::default()
                }),
            )?
        };

        let (vs, ps, input_layout) = unsafe {
            let hlsl_file = include_bytes!("shader.hlsl");
            let mut vs_blob = None;
            let mut ps_blob = None;
            let vs_blob = D3DCompile(
                hlsl_file.as_ptr() as _,
                hlsl_file.len(),
                windows::s!("shader.hlsl"),
                None,
                None,
                windows::s!("vs_main"),
                windows::s!("vs_5_0"),
                0,
                0,
                &mut vs_blob,
                None,
            )
                .map(|_| vs_blob.unwrap())?;
            let ps_blob = D3DCompile(
                hlsl_file.as_ptr() as _,
                hlsl_file.len(),
                windows::s!("shader.hlsl"),
                None,
                None,
                windows::s!("ps_main"),
                windows::s!("ps_5_0"),
                0,
                0,
                &mut ps_blob,
                None,
            )
                .map(|_| ps_blob.unwrap())?;
            let vs_blob = std::slice::from_raw_parts(
                vs_blob.GetBufferPointer() as *const u8,
                vs_blob.GetBufferSize(),
            );
            let ps_blob = std::slice::from_raw_parts(
                ps_blob.GetBufferPointer() as *const u8,
                ps_blob.GetBufferSize(),
            );
            let vs = d3d.device.CreateVertexShader(&vs_blob, None)?;
            let ps = d3d.device.CreatePixelShader(&ps_blob, None)?;
            let descs = [
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: windows::s!("POSITION"),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32B32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: 0,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: windows::s!("TEXCOORD"),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: D3D11_APPEND_ALIGNED_ELEMENT,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
            ];
            let input_layout = d3d.device.CreateInputLayout(&descs, vs_blob)?;
            (vs, ps, input_layout)
        };

        Ok(Self {
            vertex_buffer,
            index_buffer,
            vertex_shader: vs,
            pixel_shader: ps,
            input_layout,
        })
    }

    pub fn bind(&self, d3d: &Direct3D) {
        unsafe {
            d3d.context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            d3d.context.IASetInputLayout(&self.input_layout);
            d3d.context.IASetIndexBuffer(&self.index_buffer, DXGI_FORMAT_R32_UINT, 0);
            d3d.context.IASetVertexBuffers(
                0,
                1,
                Some([Some(self.vertex_buffer.clone())].as_mut_ptr()),
                Some([size_of::<Vertex>() as u32].as_ptr()),
                Some([0].as_ptr()),
            );
            d3d.context.VSSetShader(&self.vertex_shader, None);
            d3d.context.PSSetShader(&self.pixel_shader, None);
        }
    }

    pub fn draw(&self, d3d: &Direct3D, sampler: &ID3D11SamplerState, texture: &ID3D11ShaderResourceView) {
        unsafe {
            d3d.context.PSSetSamplers(0, Some(&[Some(sampler.clone())]));
            d3d.context.PSSetShaderResources(0, Some(&[Some(texture.clone())]));
            d3d.context.DrawIndexed(INDICES.len() as _, 0, 0);
        }

    }

}