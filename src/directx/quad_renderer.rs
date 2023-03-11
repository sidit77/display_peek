use std::mem::size_of;
use windows::Win32::Graphics::Direct3D11::{D3D11_APPEND_ALIGNED_ELEMENT, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_INDEX_BUFFER, D3D11_BIND_VERTEX_BUFFER, D3D11_BUFFER_DESC, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA, D3D11_SAMPLER_DESC, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, ID3D11Buffer, ID3D11InputLayout, ID3D11PixelShader, ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11VertexShader};
use anyhow::Result;
use glam::Mat4;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R32_UINT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32G32B32_FLOAT};
use crate::directx::Direct3D;
use crate::utils::make_resource;

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
    Vertex::new([0.0, 1.0, 0.0], [0.0, 1.0]),
    Vertex::new([1.0, 1.0, 0.0], [1.0, 1.0]),
    Vertex::new([0.0, 0.0, 0.0], [0.0, 0.0]),
    Vertex::new([1.0, 0.0, 0.0], [1.0, 0.0]),
];

const INDICES: [u32; 6] = [0, 2, 1, 3, 1, 2];

pub struct QuadRenderer {
    vertex_buffer: ID3D11Buffer,
    index_buffer: ID3D11Buffer,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    input_layout: ID3D11InputLayout,
    sampler: ID3D11SamplerState,
    constant_buffer: ID3D11Buffer
}

impl QuadRenderer {

    pub fn new(d3d: &Direct3D) -> Result<Self>{
        let vertex_buffer = make_resource(|ptr| unsafe {
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
                ptr
            )
        })?;
        let index_buffer = make_resource(|ptr| unsafe {
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
                ptr
            )
        })?;


        let vs_blob = include_bytes!(concat!(env!("OUT_DIR"), "/shader.vs_blob"));
        let ps_blob = include_bytes!(concat!(env!("OUT_DIR"), "/shader.ps_blob"));
        let vs = make_resource(|ptr| unsafe {
            d3d.device.CreateVertexShader(vs_blob, None, ptr)
        })?;
        let ps = make_resource(|ptr| unsafe {
            d3d.device.CreatePixelShader(ps_blob, None,ptr)
        })?;
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
        let input_layout = make_resource(|ptr| unsafe {
            d3d.device.CreateInputLayout(&descs, vs_blob, ptr)
        })?;

        let constant_buffer = make_resource(|ptr| unsafe {
            d3d.device.CreateBuffer(&D3D11_BUFFER_DESC {
                ByteWidth: size_of::<Mat4>() as _,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER,
                ..Default::default()
            },
            None,
            ptr
            )
        })?;

        let sampler = make_resource(|ptr| unsafe {
            d3d.device.CreateSamplerState(&D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                MinLOD: f32::MIN,
                MaxLOD: f32::MAX,
                MaxAnisotropy: 1,
                MipLODBias: 0.0,
                ..Default::default()
            }, ptr)
        })?;

        Ok(Self {
            vertex_buffer,
            index_buffer,
            vertex_shader: vs,
            pixel_shader: ps,
            input_layout,
            sampler,
            constant_buffer,
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
            d3d.context.VSSetConstantBuffers(0, Some(&[self.constant_buffer.clone()]));
            d3d.context.PSSetShader(&self.pixel_shader, None);
            d3d.context.PSSetSamplers(0, Some(&[self.sampler.clone()]));
        }
    }

    pub fn draw(&self, d3d: &Direct3D, transform: Mat4, texture: &ID3D11ShaderResourceView) {
        unsafe {
            let transposed = transform.transpose();
            let ptr = transposed.as_ref().as_ptr() as _;
            d3d.context.UpdateSubresource(&self.constant_buffer, 0, None, ptr, 0, 0);
            d3d.context.PSSetShaderResources(0, Some(&[texture.clone()]));
            d3d.context.DrawIndexed(INDICES.len() as _, 0, 0);
        }

    }

}