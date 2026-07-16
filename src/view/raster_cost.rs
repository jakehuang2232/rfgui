//! Shared conservative byte accounting for retained raster textures.

use crate::view::frame_graph::TextureDesc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CostConfidence {
    Exact,
    ConservativeUpperBound,
    #[default]
    Unknown,
}

impl CostConfidence {
    pub(crate) fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::ConservativeUpperBound, _) | (_, Self::ConservativeUpperBound) => {
                Self::ConservativeUpperBound
            }
            _ => Self::Exact,
        }
    }

    pub(crate) fn budget_usable(self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DescriptorPayloadBytes {
    pub(crate) bytes: u64,
    pub(crate) confidence: CostConfidence,
}

pub(crate) fn texture_desc_payload_bytes(desc: &TextureDesc) -> DescriptorPayloadBytes {
    raster_payload_bytes(
        desc.width().max(1),
        desc.height().max(1),
        desc.format(),
        desc.sample_count().max(1),
    )
}

pub(crate) fn raster_payload_bytes(
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> DescriptorPayloadBytes {
    let (bytes_per_texel, confidence) = match format {
        wgpu::TextureFormat::R8Unorm => (1, CostConfidence::Exact),
        wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb
        | wgpu::TextureFormat::Bgra8Unorm
        | wgpu::TextureFormat::Bgra8UnormSrgb => (4, CostConfidence::Exact),
        // WebGPU leaves the physical representation implementation-defined.
        // D32S8 is a possible backing, so 8 bytes/texel is a safe upper bound.
        wgpu::TextureFormat::Depth24PlusStencil8 => (8, CostConfidence::ConservativeUpperBound),
        wgpu::TextureFormat::Rgba16Float => (8, CostConfidence::Exact),
        _ => (0, CostConfidence::Unknown),
    };
    if bytes_per_texel == 0 {
        return DescriptorPayloadBytes {
            bytes: 0,
            confidence: CostConfidence::Unknown,
        };
    }
    // The pool owns one resolve texture plus an N-sample attachment for MSAA.
    let copies = if sample_count > 1 {
        1u64.saturating_add(sample_count as u64)
    } else {
        1
    };
    DescriptorPayloadBytes {
        bytes: (width as u64)
            .saturating_mul(height as u64)
            .saturating_mul(bytes_per_texel)
            .saturating_mul(copies),
        confidence,
    }
}
