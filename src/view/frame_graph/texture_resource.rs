use super::slot::ResourceType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureDesc {
    width: u32,
    height: u32,
    origin_x: u32,
    origin_y: u32,
    format: wgpu::TextureFormat,
    dimension: wgpu::TextureDimension,
    usage: wgpu::TextureUsages,
    sample_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub(crate) u32);

#[derive(Clone, Copy)]
pub struct TextureResource;

impl ResourceType for TextureResource {
    type Handle = TextureHandle;
}

impl TextureDesc {
    pub fn new(
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        dimension: wgpu::TextureDimension,
    ) -> Self {
        Self {
            width,
            height,
            origin_x: 0,
            origin_y: 0,
            format,
            dimension,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            sample_count: 1,
        }
    }

    pub fn with_usage(mut self, usage: wgpu::TextureUsages) -> Self {
        self.usage = usage;
        self
    }

    pub fn with_origin(mut self, origin_x: u32, origin_y: u32) -> Self {
        self.origin_x = origin_x;
        self.origin_y = origin_y;
        self
    }

    pub fn with_sample_count(mut self, sample_count: u32) -> Self {
        self.sample_count = sample_count.max(1);
        self
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn origin(&self) -> (u32, u32) {
        (self.origin_x, self.origin_y)
    }

    pub fn dimension(&self) -> wgpu::TextureDimension {
        self.dimension
    }

    pub fn usage(&self) -> wgpu::TextureUsages {
        self.usage
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }
}
