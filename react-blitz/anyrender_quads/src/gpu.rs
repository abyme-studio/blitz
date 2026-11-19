//! The device side: four pipelines, the frame's instances in storage buffers, the atlas's two
//! pages, a texture per opacity layer from a pool, and one render pass for the frame plus one per
//! layer.

use std::collections::HashMap;
use std::time::Instant;

use wgpu::util::DeviceExt as _;

use crate::frame_times;
use crate::atlas::{PAGE, Page};
use crate::scene::{AtlasSprite, Draw, Quad, Recording, Shadow, Sprite};

pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub(crate) struct Gpu {
    quad: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
    sprite: wgpu::RenderPipeline,
    atlas_sprite: wgpu::RenderPipeline,
    instances_layout: wgpu::BindGroupLayout,
    layer_layout: wgpu::BindGroupLayout,
    atlas_layout: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    quads: Instances,
    shadows: Instances,
    sprites: Instances,
    atlas_sprites: Instances,
    /// The atlas's pages: coverage, and premultiplied colour.
    mono: wgpu::Texture,
    color: wgpu::Texture,
    /// Images with a texture of their own, by id.
    images: HashMap<u64, wgpu::Texture>,
    sampler: wgpu::Sampler,
    /// Layer textures, the device's size, reused frame to frame.
    pool: Vec<wgpu::TextureView>,
    pool_size: (u32, u32),
}

/// A storage buffer of one kind of instance, grown as a frame needs.
struct Instances {
    buffer: wgpu::Buffer,
    label: &'static str,
}

impl Instances {
    fn new(device: &wgpu::Device, label: &'static str, size: u64) -> Self {
        Self { buffer: Self::make(device, label, size), label }
    }

    fn make(device: &wgpu::Device, label: &'static str, size: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: size.next_power_of_two().max(256),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8]) {
        if bytes.len() as u64 > self.buffer.size() {
            self.buffer = Self::make(device, self.label, bytes.len() as u64);
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytes);
        }
    }
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

impl Gpu {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("anyrender_quads"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.wgsl").into()),
        });
        let instances_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quads instances"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage_entry(1),
                storage_entry(2),
                storage_entry(3),
                storage_entry(4),
            ],
        });
        let texture_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quads atlas"),
            entries: &[
                texture_entry(1),
                texture_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layer_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quads layer"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let plain = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quads"),
            bind_group_layouts: &[Some(&instances_layout)],
            immediate_size: 0,
        });
        let with_layer = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quads sprites"),
            bind_group_layouts: &[Some(&instances_layout), Some(&layer_layout)],
            immediate_size: 0,
        });
        let with_atlas = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quads atlas sprites"),
            bind_group_layouts: &[Some(&instances_layout), Some(&atlas_layout)],
            immediate_size: 0,
        });
        let pipeline = |label: &str, layout: &wgpu::PipelineLayout, vs: &str, fs: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let quad = pipeline("quads", &plain, "vs_quad", "fs_quad");
        let shadow = pipeline("shadows", &plain, "vs_shadow", "fs_shadow");
        let sprite = pipeline("sprites", &with_layer, "vs_sprite", "fs_sprite");
        let atlas_sprite = pipeline("atlas sprites", &with_atlas, "vs_atlas_sprite", "fs_atlas_sprite");
        let page = |label: &str, format: wgpu::TextureFormat| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: PAGE, height: PAGE, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("quads atlas"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quads globals"),
            contents: &[0u8; 16],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            quad,
            shadow,
            sprite,
            atlas_sprite,
            instances_layout,
            layer_layout,
            atlas_layout,
            globals,
            quads: Instances::new(device, "quads", 64 * size_of::<Quad>() as u64),
            shadows: Instances::new(device, "shadows", 16 * size_of::<Shadow>() as u64),
            sprites: Instances::new(device, "sprites", 16 * size_of::<Sprite>() as u64),
            atlas_sprites: Instances::new(device, "atlas sprites", 256 * size_of::<AtlasSprite>() as u64),
            mono: page("quads atlas coverage", wgpu::TextureFormat::R8Unorm),
            color: page("quads atlas colour", FORMAT),
            images: HashMap::new(),
            sampler,
            pool: Vec::new(),
            pool_size: (0, 0),
        }
    }

    /// Draws `recording` into `target`, cleared to `clear` first. The instances' upload and the
    /// passes' recording and submit are `render` in the frame's times.
    pub(crate) fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        recording: &mut Recording,
        target: &wgpu::TextureView,
        size: (u32, u32),
        clear: wgpu::Color,
    ) {
        let started = Instant::now();
        self.upload(device, queue, recording);
        let recording = &*recording;
        queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]));
        self.quads.write(device, queue, bytemuck::cast_slice(&recording.quads));
        self.shadows.write(device, queue, bytemuck::cast_slice(&recording.shadows));
        self.sprites.write(device, queue, bytemuck::cast_slice(&recording.sprites));
        self.atlas_sprites.write(device, queue, bytemuck::cast_slice(&recording.atlas_sprites));
        let instances = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quads instances"),
            layout: &self.instances_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.quads.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: self.shadows.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: self.sprites.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: self.atlas_sprites.buffer.as_entire_binding() },
            ],
        });
        // The atlas's pages, and one group per image with a texture of its own drawn this frame.
        let mono = self.mono.create_view(&Default::default());
        let atlas_group = |color: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("quads atlas"),
                layout: &self.atlas_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&mono) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(color) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            })
        };
        let mut atlas_groups = HashMap::new();
        atlas_groups.insert(Page::Color, atlas_group(&self.color.create_view(&Default::default())));
        for id in sprite_images(recording) {
            if let Some(texture) = self.images.get(&id) {
                atlas_groups.insert(Page::Image(id), atlas_group(&texture.create_view(&Default::default())));
            }
        }

        // A texture per layer, the device's size.
        if self.pool_size != size {
            self.pool.clear();
            self.pool_size = size;
        }
        while self.pool.len() < recording.layers.len() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("quads layer"),
                size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.pool.push(texture.create_view(&Default::default()));
        }
        let layer_groups: Vec<wgpu::BindGroup> = (0..recording.layers.len())
            .map(|i| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("quads layer"),
                    layout: &self.layer_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.pool[i]),
                    }],
                })
            })
            .collect();

        let mut encoder = device.create_command_encoder(&Default::default());
        let mut passes = 0;
        let mut draws = 0;
        // Each layer's pass before the pass that composites it.
        let mut order = Vec::new();
        layers_in_order(recording, &recording.root, &mut order);
        for layer in order {
            self.pass(&mut encoder, &self.pool[layer], wgpu::Color::TRANSPARENT, &recording.layers[layer].draws, &instances, &layer_groups, &atlas_groups, &mut draws);
            passes += 1;
        }
        self.pass(&mut encoder, target, clear, &recording.root, &instances, &layer_groups, &atlas_groups, &mut draws);
        passes += 1;
        queue.submit([encoder.finish()]);
        let took = started.elapsed();
        frame_times::add(|t| {
            t.render += took;
            t.draws += draws;
            t.passes += passes;
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clear: wgpu::Color,
        list: &[Draw],
        instances: &wgpu::BindGroup,
        layer_groups: &[wgpu::BindGroup],
        atlas_groups: &HashMap<Page, wgpu::BindGroup>,
        draws: &mut u32,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("quads"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, instances, &[]);
        for draw in list {
            match draw {
                Draw::Quads(range) => {
                    pass.set_pipeline(&self.quad);
                    pass.draw(0..4, range.clone());
                }
                Draw::Shadows(range) => {
                    pass.set_pipeline(&self.shadow);
                    pass.draw(0..4, range.clone());
                }
                Draw::Layer { layer, sprite } => {
                    pass.set_pipeline(&self.sprite);
                    pass.set_bind_group(1, &layer_groups[*layer], &[]);
                    pass.draw(0..4, *sprite..*sprite + 1);
                }
                Draw::Sprites { range, page } => {
                    // Glyphs from the single-channel page sample it beside the colour page.
                    let group = if *page == Page::Mono { Page::Color } else { *page };
                    let Some(group) = atlas_groups.get(&group) else { continue };
                    pass.set_pipeline(&self.atlas_sprite);
                    pass.set_bind_group(1, group, &[]);
                    pass.draw(0..4, range.clone());
                }
            }
            *draws += 1;
        }
    }
}

impl Gpu {
    /// Writes the atlas's new texels, making a texture for each new image that has its own.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, recording: &mut Recording) {
        for upload in recording.atlas.uploads.drain(..) {
            let [x, y, w, h] = upload.rect;
            if w == 0 || h == 0 {
                continue;
            }
            let texture = match upload.page {
                Page::Mono => &self.mono,
                Page::Color => &self.color,
                Page::Image(id) => self.images.entry(id).or_insert_with(|| {
                    device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("quads image"),
                        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: FORMAT,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    })
                }),
            };
            let texel = if upload.page == Page::Mono { 1 } else { 4 };
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &upload.bytes,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * texel), rows_per_image: Some(h) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
    }
}

/// The images with a texture of their own that the frame's sprites draw.
fn sprite_images(recording: &Recording) -> Vec<u64> {
    let mut ids = Vec::new();
    let mut scan = |list: &[Draw]| {
        for draw in list {
            if let Draw::Sprites { page: Page::Image(id), .. } = draw
                && !ids.contains(id)
            {
                ids.push(*id);
            }
        }
    };
    scan(&recording.root);
    for layer in &recording.layers {
        scan(&layer.draws);
    }
    ids
}

/// The layers `list` composites, each after the layers it composites itself.
fn layers_in_order(recording: &Recording, list: &[Draw], order: &mut Vec<usize>) {
    for draw in list {
        if let Draw::Layer { layer, .. } = draw {
            layers_in_order(recording, &recording.layers[*layer].draws, order);
            order.push(*layer);
        }
    }
}
