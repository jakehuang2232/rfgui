use crate::view::frame_graph::{
    FrameResourceContext, GraphicsColorAttachmentOps, GraphicsPassBuilder, GraphicsPassMergePolicy,
    PrepareContext,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    GraphicsPassContext as RenderPassContext, logical_scissor_to_target_physical,
    render_target_format, render_target_origin, render_target_sample_count, resolve_texture_ref,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use crate::view::text_layout::{TextGlyph, TextLayout};
use parley::FontData as ParleyFontData;
use rustc_hash::FxHashMap;
#[cfg(test)]
use std::collections::hash_map::DefaultHasher;
#[cfg(test)]
use std::hash::{Hash, Hasher};
use std::cell::RefCell;
use std::sync::Arc;
use swash::FontRef as SwashFontRef;
use swash::scale::image::{Content as SwashRasterContent, Image as SwashRasterImage};
use swash::scale::{
    Render as SwashRender, ScaleContext as SwashScaleContext, Source, StrikeWith as SwashStrikeWith,
};
use swash::zeno::{Format as SwashFormat, Vector as SwashVector};

pub struct TextPass {
    params: TextPassParams,
    prepared: Option<TextPreparedState>,
    input: TextInput,
    output: TextOutput,
}

#[derive(Clone)]
pub struct TextPassFragment {
    pub content: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
    pub opacity: f32,
    pub(crate) text_layout: Option<Arc<TextLayout>>,
}

pub struct TextPassParams {
    pub fragments: Vec<TextPassFragment>,
    pub font_size: f32,
    pub line_height: f32,
    pub font_weight: u16,
    pub font_families: Vec<String>,
    pub allow_wrap: bool,
    pub scissor_rect: Option<[u32; 4]>,
    pub stencil_clip_id: Option<u8>,
}

impl TextPassParams {
    pub fn single_fragment(
        fragment: TextPassFragment,
        font_size: f32,
        line_height: f32,
        font_weight: u16,
        font_families: Vec<String>,
        allow_wrap: bool,
        scissor_rect: Option<[u32; 4]>,
        stencil_clip_id: Option<u8>,
    ) -> Self {
        Self {
            fragments: vec![fragment],
            font_size,
            line_height,
            font_weight,
            font_families,
            allow_wrap,
            scissor_rect,
            stencil_clip_id,
        }
    }
}

#[derive(Default)]
pub struct TextInput {
    pub pass_context: RenderPassContext,
}

#[derive(Default)]
pub struct TextOutput {
    pub render_target: RenderTargetOut,
}

impl TextPass {
    pub fn new(params: TextPassParams, input: TextInput, output: TextOutput) -> Self {
        Self {
            params,
            prepared: None,
            input,
            output,
        }
    }
}

struct TextPreparedState {
    renderer_key: TextRendererKey,
    globals_bind_group: wgpu::BindGroup,
    screen_buffer: wgpu::Buffer,
    fragment_buffer: wgpu::Buffer,
    mask_draw: Option<PreparedTextDraw>,
    color_draw: Option<PreparedTextDraw>,
    scissor_rect: Option<[u32; 4]>,
    stencil_clip_id: Option<u8>,
}

impl Drop for TextPreparedState {
    fn drop(&mut self) {
        self.screen_buffer.destroy();
        self.fragment_buffer.destroy();
    }
}

struct PreparedTextDraw {
    vertex_buffer: wgpu::Buffer,
    instance_count: u32,
    atlas_texture: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    atlas_bind_group: wgpu::BindGroup,
}

impl Drop for PreparedTextDraw {
    fn drop(&mut self) {
        self.vertex_buffer.destroy();
        self.atlas_texture.destroy();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TextRendererKey {
    format: wgpu::TextureFormat,
    sample_count: u32,
    stencil_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TextPipelineKind {
    Mask,
    Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum AtlasKind {
    Mask,
    Color,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUniform {
    screen_size: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct FragmentUniform {
    origin: [f32; 2],
    clip_min: [f32; 2],
    clip_max: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct TextGlyphInstance {
    local_pos: [f32; 2],
    size: [f32; 2],
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    color: [f32; 4],
    opacity: f32,
    fragment_index: u32,
}

struct PendingGlyphInstance {
    kind: AtlasKind,
    local_pos: [f32; 2],
    size: [f32; 2],
    image: SwashRasterImage,
    color: [f32; 4],
    opacity: f32,
    fragment_index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TextRasterKey {
    font_blob_id: u64,
    font_index: u32,
    glyph_id: u32,
    font_size_bits: u32,
    scale_factor_bits: u32,
    normalized_coords_hash: u64,
}

struct CachedRasterImage {
    image: SwashRasterImage,
    last_used_frame: u64,
}

#[derive(Default)]
struct TextResources {
    screen_layout: Option<wgpu::BindGroupLayout>,
    atlas_layout: Option<wgpu::BindGroupLayout>,
    sampler: Option<wgpu::Sampler>,
    pipelines: FxHashMap<(TextRendererKey, TextPipelineKind), wgpu::RenderPipeline>,
    raster_cache: FxHashMap<TextRasterKey, CachedRasterImage>,
    frame_epoch: u64,
    scale_context: SwashScaleContext,
}

thread_local! {
    static TEXT_RESOURCES: RefCell<TextResources> = RefCell::new(TextResources::default());
}

impl GraphicsPass for TextPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        if builder.texture_target(&self.output.render_target).is_some() {
            builder.write_color(
                &self.output.render_target,
                GraphicsColorAttachmentOps::load(),
            );
        } else {
            builder.write_surface_color(GraphicsColorAttachmentOps::load());
        }

        if self.input.pass_context.uses_depth_stencil {
            builder.read_output_depth();
            builder.read_output_stencil();
        }
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        self.prepared = prepare_text_pass(&self.params, &self.input, &self.output, ctx);
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        let Some(prepared) = self.prepared.as_ref() else {
            return;
        };
        if let Some(scissor) = prepared.scissor_rect {
            ctx.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
        }
        if let Some(reference) = prepared.stencil_clip_id {
            ctx.set_stencil_reference(reference as u32);
        }

        draw_prepared_text(
            ctx,
            prepared,
            TextPipelineKind::Mask,
            prepared.mask_draw.as_ref(),
        );
        draw_prepared_text(
            ctx,
            prepared,
            TextPipelineKind::Color,
            prepared.color_draw.as_ref(),
        );
    }

    fn name(&self) -> &'static str {
        "TextPass"
    }
}

fn prepare_text_pass(
    params: &TextPassParams,
    input: &TextInput,
    output: &TextOutput,
    ctx: &mut PrepareContext<'_, '_>,
) -> Option<TextPreparedState> {
    if params.fragments.is_empty() {
        return None;
    }

    let target_handle = output.render_target.handle();
    let (device, queue, surface_format, surface_size, scale_factor) = {
        let viewport = ctx.viewport();
        (
            viewport.device()?.clone(),
            viewport.queue()?.clone(),
            viewport.surface_format(),
            viewport.surface_size(),
            viewport.scale_factor().max(0.0001),
        )
    };

    let target_format = target_handle
        .and_then(|handle| render_target_format(ctx, handle))
        .unwrap_or(surface_format);
    let sample_count = target_handle
        .and_then(|handle| render_target_sample_count(ctx, handle))
        .unwrap_or(1)
        .max(1);
    let target = resolve_texture_ref(target_handle, ctx, surface_size, None);
    let target_origin = target_handle
        .and_then(|handle| render_target_origin(ctx, handle))
        .unwrap_or((0, 0));
    let scissor_rect = params
        .scissor_rect
        .or(input.pass_context.scissor_rect)
        .and_then(|rect| {
            logical_scissor_to_target_physical(
                ctx.viewport(),
                rect,
                target_origin,
                target.physical_size,
            )
        });
    let stencil_clip_id = params
        .stencil_clip_id
        .or(input.pass_context.stencil_clip_id);

    let renderer_key = TextRendererKey {
        format: target_format,
        sample_count,
        stencil_enabled: input.pass_context.uses_depth_stencil,
    };

    let mut fragments = Vec::with_capacity(params.fragments.len());
    let mut pending = Vec::new();
    for fragment in params.fragments.iter() {
        if fragment.content.is_empty() || fragment.opacity <= 0.0 {
            continue;
        }
        let width = fragment.width.max(1.0);
        let height = fragment.height.max(1.0);
        let origin = [
            fragment.x * scale_factor - target_origin.0 as f32 + target.logical_origin.0 as f32,
            fragment.y * scale_factor - target_origin.1 as f32 + target.logical_origin.1 as f32,
        ];
        fragments.push(FragmentUniform {
            origin,
            clip_min: origin,
            clip_max: [
                origin[0] + width * scale_factor,
                origin[1] + height * scale_factor,
            ],
            _pad: [0.0, 0.0],
        });
        let Some(layout) = fragment.text_layout.as_ref() else {
            continue;
        };
        let active_fragment_index = (fragments.len() - 1) as u32;
        TEXT_RESOURCES.with(|slot| {
            let mut resources = slot.borrow_mut();
            let frame_epoch = resources.frame_epoch;
            let TextResources {
                scale_context,
                raster_cache,
                ..
            } = &mut *resources;
            collect_fragment_glyphs(
                scale_context,
                raster_cache,
                frame_epoch,
                layout,
                origin,
                fragment.color,
                fragment.opacity,
                active_fragment_index,
                scale_factor,
                &mut pending,
            );
        });
    }

    if fragments.is_empty() {
        return None;
    }

    let screen_uniform = ScreenUniform {
        screen_size: [
            target.physical_size.0.max(1) as f32,
            target.physical_size.1.max(1) as f32,
        ],
        _pad: [0.0, 0.0],
    };
    let screen_buffer = super::create_transient_buffer(
        &device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("Text Screen Uniform Buffer"),
            contents: bytemuck::bytes_of(&screen_uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        },
    );
    let fragment_buffer = super::create_transient_buffer(
        &device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("Text Fragment Storage Buffer"),
            contents: bytemuck::cast_slice(&fragments),
            usage: wgpu::BufferUsages::STORAGE,
        },
    );

    let (globals_bind_group, mask_draw, color_draw) = TEXT_RESOURCES.with(|slot| {
        let mut resources = slot.borrow_mut();
        resources.ensure_common(&device);
        let screen_layout = resources
            .screen_layout
            .as_ref()
            .expect("screen bind group layout initialized");
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Globals Bind Group"),
            layout: screen_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: screen_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: fragment_buffer.as_entire_binding(),
                },
            ],
        });
        let mask_draw = build_prepared_draw(
            &device,
            &queue,
            &mut resources,
            AtlasKind::Mask,
            pending.iter().filter(|glyph| glyph.kind == AtlasKind::Mask),
        );
        let color_draw = build_prepared_draw(
            &device,
            &queue,
            &mut resources,
            AtlasKind::Color,
            pending.iter().filter(|glyph| glyph.kind == AtlasKind::Color),
        );
        resources.ensure_pipeline(&device, renderer_key, TextPipelineKind::Mask);
        resources.ensure_pipeline(&device, renderer_key, TextPipelineKind::Color);
        (globals_bind_group, mask_draw, color_draw)
    });

    if mask_draw.is_none() && color_draw.is_none() {
        screen_buffer.destroy();
        fragment_buffer.destroy();
        return None;
    }

    Some(TextPreparedState {
        renderer_key,
        globals_bind_group,
        screen_buffer,
        fragment_buffer,
        mask_draw,
        color_draw,
        scissor_rect,
        stencil_clip_id,
    })
}

fn collect_fragment_glyphs(
    scale_context: &mut SwashScaleContext,
    raster_cache: &mut FxHashMap<TextRasterKey, CachedRasterImage>,
    frame_epoch: u64,
    layout: &TextLayout,
    fragment_origin: [f32; 2],
    color: [f32; 4],
    opacity: f32,
    fragment_index: u32,
    scale_factor: f32,
    out: &mut Vec<PendingGlyphInstance>,
) {
    for line in layout.lines() {
        let baseline_y = line.y + line.baseline;
        for glyph in line.glyphs {
            let Some(image) =
                rasterize_glyph(scale_context, raster_cache, frame_epoch, &glyph, scale_factor)
            else {
                continue;
            };
            let width = image.placement.width.max(1) as f32;
            let height = image.placement.height.max(1) as f32;
            if width <= 0.0 || height <= 0.0 || image.data.is_empty() {
                continue;
            }
            let kind = match image.content {
                SwashRasterContent::Color => AtlasKind::Color,
                SwashRasterContent::Mask | SwashRasterContent::SubpixelMask => AtlasKind::Mask,
            };
            let local_pos = snap_text_local_pos(
                fragment_origin,
                [
                    (line.x + glyph.x) * scale_factor + image.placement.left as f32,
                    (baseline_y + glyph.y) * scale_factor - image.placement.top as f32,
                ],
            );
            out.push(PendingGlyphInstance {
                kind,
                local_pos,
                size: [width, height],
                image,
                color,
                opacity,
                fragment_index,
            });
        }
    }
}

fn snap_text_local_pos(fragment_origin: [f32; 2], local_pos: [f32; 2]) -> [f32; 2] {
    [
        text_render_trunc(fragment_origin[0] + local_pos[0]) - fragment_origin[0],
        text_render_trunc(fragment_origin[1] + local_pos[1]) - fragment_origin[1],
    ]
}

fn text_render_trunc(value: f32) -> f32 {
    if value.is_sign_negative() {
        value.ceil()
    } else {
        value.floor()
    }
}

fn rasterize_glyph(
    scale_context: &mut SwashScaleContext,
    raster_cache: &mut FxHashMap<TextRasterKey, CachedRasterImage>,
    frame_epoch: u64,
    glyph: &TextGlyph,
    scale_factor: f32,
) -> Option<SwashRasterImage> {
    let font_data = glyph.font_data.as_ref()?;
    let key = TextRasterKey {
        font_blob_id: font_data.data.id(),
        font_index: font_data.index,
        glyph_id: glyph.id,
        font_size_bits: glyph.font_size.to_bits(),
        scale_factor_bits: scale_factor.to_bits(),
        normalized_coords_hash: glyph.normalized_coords_hash,
    };
    if let Some(entry) = raster_cache.get_mut(&key) {
        entry.last_used_frame = frame_epoch;
        return Some(entry.image.clone());
    }

    let font_ref = swash_font_ref(font_data)?;
    let mut scaler = scale_context
        .builder(font_ref)
        .size((glyph.font_size * scale_factor).max(1.0))
        .hint(false)
        .build();
    let sources = [
        Source::ColorBitmap(SwashStrikeWith::BestFit),
        Source::ColorOutline(0),
        Source::Bitmap(SwashStrikeWith::BestFit),
        Source::Outline,
    ];
    let mut image = SwashRasterImage::new();
    let rendered = SwashRender::new(&sources)
        .format(SwashFormat::Alpha)
        .offset(SwashVector::new(0.0, 0.0))
        .render_into(&mut scaler, glyph.id as u16, &mut image);
    if !rendered {
        return None;
    }
    raster_cache.insert(
        key,
        CachedRasterImage {
            image: image.clone(),
            last_used_frame: frame_epoch,
        },
    );
    Some(image)
}

fn swash_font_ref(font_data: &ParleyFontData) -> Option<SwashFontRef<'_>> {
    SwashFontRef::from_index(font_data.data.data(), font_data.index as usize)
}

fn build_prepared_draw<'a>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &mut TextResources,
    atlas_kind: AtlasKind,
    glyphs: impl Iterator<Item = &'a PendingGlyphInstance>,
) -> Option<PreparedTextDraw> {
    let glyphs = glyphs.collect::<Vec<_>>();
    if glyphs.is_empty() {
        return None;
    }
    let atlas = pack_atlas(atlas_kind, glyphs.as_slice())?;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(match atlas_kind {
            AtlasKind::Mask => "Text Mask Atlas",
            AtlasKind::Color => "Text Color Atlas",
        }),
        size: wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        atlas.pixels.as_slice(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas.width.saturating_mul(4)),
            rows_per_image: Some(atlas.height),
        },
        wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let atlas_layout = resources
        .atlas_layout
        .as_ref()
        .expect("atlas bind group layout initialized");
    let sampler = resources.sampler.as_ref().expect("sampler initialized");
    let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Text Atlas Bind Group"),
        layout: atlas_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    let vertex_buffer = super::create_transient_buffer(
        device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("Text Glyph Instance Buffer"),
            contents: bytemuck::cast_slice(atlas.instances.as_slice()),
            usage: wgpu::BufferUsages::VERTEX,
        },
    );
    Some(PreparedTextDraw {
        vertex_buffer,
        instance_count: atlas.instances.len() as u32,
        atlas_texture: texture,
        atlas_view: view,
        atlas_bind_group,
    })
}

struct PackedAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    instances: Vec<TextGlyphInstance>,
}

fn pack_atlas(atlas_kind: AtlasKind, glyphs: &[&PendingGlyphInstance]) -> Option<PackedAtlas> {
    const PADDING: u32 = 1;
    const MAX_WIDTH: u32 = 2048;

    let mut x = PADDING;
    let mut y = PADDING;
    let mut row_height = 0;
    let mut placements = Vec::with_capacity(glyphs.len());
    let mut atlas_width = PADDING;
    for glyph in glyphs {
        let w = glyph.image.placement.width.max(1);
        let h = glyph.image.placement.height.max(1);
        if x + w + PADDING > MAX_WIDTH && x > PADDING {
            y += row_height + PADDING;
            x = PADDING;
            row_height = 0;
        }
        placements.push((x, y, w, h));
        atlas_width = atlas_width.max(x + w + PADDING);
        x += w + PADDING;
        row_height = row_height.max(h);
    }
    let atlas_height = (y + row_height + PADDING).max(1);
    let atlas_width = atlas_width.max(1);
    let mut pixels = vec![0_u8; atlas_width as usize * atlas_height as usize * 4];
    let mut instances = Vec::with_capacity(glyphs.len());

    for (glyph, (dst_x, dst_y, width, height)) in glyphs.iter().zip(placements) {
        copy_glyph_to_atlas(
            atlas_kind,
            &glyph.image,
            &mut pixels,
            atlas_width,
            dst_x,
            dst_y,
        );
        let uv_min = [
            dst_x as f32 / atlas_width as f32,
            dst_y as f32 / atlas_height as f32,
        ];
        let uv_max = [
            (dst_x + width) as f32 / atlas_width as f32,
            (dst_y + height) as f32 / atlas_height as f32,
        ];
        instances.push(TextGlyphInstance {
            local_pos: glyph.local_pos,
            size: glyph.size,
            uv_min,
            uv_max,
            color: glyph.color,
            opacity: glyph.opacity,
            fragment_index: glyph.fragment_index,
        });
    }

    Some(PackedAtlas {
        width: atlas_width,
        height: atlas_height,
        pixels,
        instances,
    })
}

fn copy_glyph_to_atlas(
    atlas_kind: AtlasKind,
    image: &SwashRasterImage,
    pixels: &mut [u8],
    atlas_width: u32,
    dst_x: u32,
    dst_y: u32,
) {
    let width = image.placement.width;
    let height = image.placement.height;
    for row in 0..height {
        for col in 0..width {
            let dst = (((dst_y + row) * atlas_width + dst_x + col) * 4) as usize;
            match (atlas_kind, image.content) {
                (AtlasKind::Color, SwashRasterContent::Color) => {
                    let src = ((row * width + col) * 4) as usize;
                    if src + 3 < image.data.len() && dst + 3 < pixels.len() {
                        pixels[dst..dst + 4].copy_from_slice(&image.data[src..src + 4]);
                    }
                }
                (_, SwashRasterContent::Mask) => {
                    let src = (row * width + col) as usize;
                    if src < image.data.len() && dst + 3 < pixels.len() {
                        let coverage = image.data[src];
                        pixels[dst] = coverage;
                        pixels[dst + 1] = coverage;
                        pixels[dst + 2] = coverage;
                        pixels[dst + 3] = coverage;
                    }
                }
                (_, SwashRasterContent::SubpixelMask) => {
                    let src = ((row * width + col) * 4) as usize;
                    if src + 3 < image.data.len() && dst + 3 < pixels.len() {
                        let coverage = image.data[src]
                            .max(image.data[src + 1])
                            .max(image.data[src + 2]);
                        pixels[dst] = coverage;
                        pixels[dst + 1] = coverage;
                        pixels[dst + 2] = coverage;
                        pixels[dst + 3] = image.data[src + 3].max(coverage);
                    }
                }
                (AtlasKind::Mask, SwashRasterContent::Color) => {}
            }
        }
    }
}

fn draw_prepared_text(
    ctx: &mut GraphicsCtx<'_, '_, '_, '_>,
    prepared: &TextPreparedState,
    kind: TextPipelineKind,
    draw: Option<&PreparedTextDraw>,
) {
    let Some(draw) = draw else {
        return;
    };
    if draw.instance_count == 0 {
        return;
    }
    TEXT_RESOURCES.with(|slot| {
        let resources = slot.borrow();
        let Some(pipeline) = resources.pipelines.get(&(prepared.renderer_key, kind)) else {
            return;
        };
        let _ = &draw.atlas_view;
        ctx.set_pipeline(pipeline);
        ctx.set_bind_group(0, &prepared.globals_bind_group, &[]);
        ctx.set_bind_group(1, &draw.atlas_bind_group, &[]);
        ctx.set_vertex_buffer(0, draw.vertex_buffer.slice(..));
        ctx.draw(0..6, 0..draw.instance_count);
    });
}

impl TextResources {
    fn begin_frame(&mut self) {
        self.frame_epoch = self.frame_epoch.wrapping_add(1);
        self.evict_raster_cache();
    }

    fn evict_raster_cache(&mut self) {
        const MAX_RASTER_CACHE_ENTRIES: usize = 4096;
        const MAX_UNUSED_FRAMES: u64 = 180;
        let frame_epoch = self.frame_epoch;
        self.raster_cache.retain(|_, entry| {
            frame_epoch.saturating_sub(entry.last_used_frame) <= MAX_UNUSED_FRAMES
        });
        if self.raster_cache.len() <= MAX_RASTER_CACHE_ENTRIES {
            return;
        }
        let mut entries = self
            .raster_cache
            .iter()
            .map(|(key, entry)| (*key, entry.last_used_frame))
            .collect::<Vec<_>>();
        entries.sort_by_key(|(_, last_used)| *last_used);
        let remove_count = self.raster_cache.len() - MAX_RASTER_CACHE_ENTRIES;
        for (key, _) in entries.into_iter().take(remove_count) {
            self.raster_cache.remove(&key);
        }
    }

    fn ensure_common(&mut self, device: &wgpu::Device) {
        if self.screen_layout.is_none() {
            self.screen_layout = Some(device.create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("Text Globals Bind Group Layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                },
            ));
        }
        if self.atlas_layout.is_none() {
            self.atlas_layout = Some(device.create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("Text Atlas Bind Group Layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                },
            ));
        }
        if self.sampler.is_none() {
            self.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Text Atlas Sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            }));
        }
    }

    fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        key: TextRendererKey,
        kind: TextPipelineKind,
    ) {
        self.ensure_common(device);
        if self.pipelines.contains_key(&(key, kind)) {
            return;
        }
        let screen_layout = self.screen_layout.as_ref().expect("screen layout");
        let atlas_layout = self.atlas_layout.as_ref().expect("atlas layout");
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Pipeline Layout"),
            bind_group_layouts: &[Some(screen_layout), Some(atlas_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/text.wgsl").into()),
        });
        let fragment_entry = match kind {
            TextPipelineKind::Mask => "fs_mask",
            TextPipelineKind::Color => "fs_color",
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[text_glyph_vertex_layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: key.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: key.stencil_enabled.then_some(text_depth_stencil_state()),
            multisample: wgpu::MultisampleState {
                count: key.sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert((key, kind), pipeline);
    }

    fn destroy(&mut self) {
        self.pipelines.clear();
        self.raster_cache.clear();
        self.screen_layout = None;
        self.atlas_layout = None;
        self.sampler = None;
        self.scale_context = SwashScaleContext::new();
    }
}

fn text_glyph_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32x2,
        4 => Float32x4,
        5 => Float32,
        6 => Uint32
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<TextGlyphInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRS,
    }
}

fn text_depth_stencil_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            },
            back: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            },
            read_mask: 0xff,
            write_mask: 0x00,
        },
        bias: Default::default(),
    }
}

pub fn prewarm_text_pipeline(
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    sample_count: u32,
) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (device, format, sample_count);
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    TEXT_RESOURCES.with(|slot| {
        let mut resources = slot.borrow_mut();
        let regular = TextRendererKey {
            format,
            sample_count: sample_count.max(1),
            stencil_enabled: false,
        };
        let stencil = TextRendererKey {
            stencil_enabled: true,
            ..regular
        };
        resources.ensure_pipeline(device, regular, TextPipelineKind::Mask);
        resources.ensure_pipeline(device, regular, TextPipelineKind::Color);
        resources.ensure_pipeline(device, stencil, TextPipelineKind::Mask);
        resources.ensure_pipeline(device, stencil, TextPipelineKind::Color);
    });
}

pub fn clear_text_resources_cache() {
    TEXT_RESOURCES.with(|slot| slot.borrow_mut().destroy());
}

pub fn begin_text_resources_frame() {
    TEXT_RESOURCES.with(|slot| slot.borrow_mut().begin_frame());
}

#[cfg(test)]
fn signature_for_params(params: &TextPassParams) -> u64 {
    let mut hasher = DefaultHasher::new();
    params.font_size.to_bits().hash(&mut hasher);
    params.line_height.to_bits().hash(&mut hasher);
    params.font_weight.hash(&mut hasher);
    params.font_families.hash(&mut hasher);
    params.allow_wrap.hash(&mut hasher);
    params.scissor_rect.hash(&mut hasher);
    params.stencil_clip_id.hash(&mut hasher);
    for fragment in &params.fragments {
        fragment.content.hash(&mut hasher);
        fragment.x.to_bits().hash(&mut hasher);
        fragment.y.to_bits().hash(&mut hasher);
        fragment.width.to_bits().hash(&mut hasher);
        fragment.height.to_bits().hash(&mut hasher);
        for channel in fragment.color {
            channel.to_bits().hash(&mut hasher);
        }
        fragment.opacity.to_bits().hash(&mut hasher);
        if let Some(layout) = fragment.text_layout.as_ref() {
            Arc::as_ptr(layout).hash(&mut hasher);
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fragment(content: &str) -> TextPassFragment {
        TextPassFragment {
            content: content.to_string(),
            x: 1.0,
            y: 2.0,
            width: 30.0,
            height: 40.0,
            color: [1.0, 0.5, 0.25, 1.0],
            opacity: 0.75,
            text_layout: None,
        }
    }

    #[test]
    fn single_fragment_preserves_public_fields() {
        let params = TextPassParams::single_fragment(
            fragment("hello"),
            14.0,
            1.2,
            500,
            vec!["Noto Sans".to_string()],
            true,
            Some([1, 2, 3, 4]),
            Some(9),
        );
        assert_eq!(params.fragments.len(), 1);
        assert_eq!(params.fragments[0].content, "hello");
        assert_eq!(params.font_size, 14.0);
        assert_eq!(params.line_height, 1.2);
        assert_eq!(params.font_weight, 500);
        assert_eq!(params.font_families, vec!["Noto Sans"]);
        assert!(params.allow_wrap);
        assert_eq!(params.scissor_rect, Some([1, 2, 3, 4]));
        assert_eq!(params.stencil_clip_id, Some(9));
    }

    #[test]
    fn allow_wrap_participates_in_signature_without_layout_behavior() {
        let wrapped = TextPassParams::single_fragment(
            fragment("same"),
            14.0,
            1.2,
            400,
            vec!["sans-serif".to_string()],
            true,
            None,
            None,
        );
        let nowrap = TextPassParams::single_fragment(
            fragment("same"),
            14.0,
            1.2,
            400,
            vec!["sans-serif".to_string()],
            false,
            None,
            None,
        );
        assert_ne!(
            signature_for_params(&wrapped),
            signature_for_params(&nowrap)
        );
    }

    #[test]
    fn snap_text_local_pos_snaps_absolute_pixel_position() {
        let fragment_origin = [10.25, 4.75];
        let local = snap_text_local_pos(fragment_origin, [3.90, 2.40]);

        assert_eq!(fragment_origin[0] + local[0], 14.0);
        assert_eq!(fragment_origin[1] + local[1], 7.0);
    }

    #[test]
    fn text_render_trunc_moves_toward_zero() {
        assert_eq!(text_render_trunc(4.9), 4.0);
        assert_eq!(text_render_trunc(-4.9), -4.0);
    }

    #[test]
    fn text_glyph_instance_layout_matches_shader_locations() {
        assert_eq!(std::mem::size_of::<TextGlyphInstance>(), 56);
        let attrs = text_glyph_vertex_layout();
        assert_eq!(attrs.array_stride, 56);
        assert_eq!(attrs.attributes.len(), 7);
        assert_eq!(attrs.attributes[0].offset, 0);
        assert_eq!(attrs.attributes[4].offset, 32);
        assert_eq!(attrs.attributes[6].offset, 52);
    }
}
