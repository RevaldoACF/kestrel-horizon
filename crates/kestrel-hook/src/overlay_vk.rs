use crate::config::FullbrightMode;
use crate::draw::{DrawList, UiVertex};
use crate::font::FontAtlas;
use crate::hooks_vk::sync_xray_flags;
use crate::hooks_x11::{set_freecam_active, set_ui_capturing, sync_cursor_from_ui_transition, PASSIVE_KEYS_MASK};
use crate::hud::render_hud;
use crate::state::STATE;
use crate::ui::render_menu;
use std::sync::atomic::Ordering;
use ash::vk;
use std::mem::size_of;
use std::ptr;

static VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/overlay.vert.spv"));
static FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/overlay.frag.spv"));

const FRAMES_IN_FLIGHT: usize = 3;
const MAX_VERTICES_PER_FRAME: usize = 32_768;
const MAX_INDICES_PER_FRAME: usize = 65_536;

#[repr(C)]
#[derive(Clone, Copy)]
struct PushConstants {
    screen_size: [f32; 2],
    is_srgb: f32,
    _pad: f32,
}

struct FrameSlot {
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
    render_finished: vk::Semaphore,
    vb_offset: vk::DeviceSize,
    ib_offset: vk::DeviceSize,
}

struct SwapchainResources {
    swapchain: vk::SwapchainKHR,
    format: vk::Format,
    extent: vk::Extent2D,
    is_srgb: bool,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    render_pass: vk::RenderPass,
    pipeline: vk::Pipeline,
}

pub struct VulkanOverlay {
    device: ash::Device,
    mem_props: vk::PhysicalDeviceMemoryProperties,
    queue_family: u32,

    cmd_pool: vk::CommandPool,
    slots: Vec<FrameSlot>,
    frame_cursor: usize,

    vbuf: vk::Buffer,
    vmem: vk::DeviceMemory,
    vptr: *mut u8,

    ibuf: vk::Buffer,
    imem: vk::DeviceMemory,
    iptr: *mut u8,

    lm_buf: vk::Buffer,
    lm_mem: vk::DeviceMemory,
    lm_ptr: *mut u8,

    atlas: FontAtlas,
    atlas_img: vk::Image,
    atlas_mem: vk::DeviceMemory,
    atlas_view: vk::ImageView,
    atlas_sampler: vk::Sampler,

    desc_layout: vk::DescriptorSetLayout,
    desc_pool: vk::DescriptorPool,
    desc_set: vk::DescriptorSet,
    pipe_layout: vk::PipelineLayout,
    vert_mod: vk::ShaderModule,
    frag_mod: vk::ShaderModule,

    swapchain_res: Option<SwapchainResources>,
    draw_list: DrawList,
    was_capturing: bool,
}

unsafe impl Send for VulkanOverlay {}
unsafe impl Sync for VulkanOverlay {}

impl VulkanOverlay {
    pub unsafe fn new(
        instance: &ash::Instance,
        phys: vk::PhysicalDevice,
        device: ash::Device,
        queue_family: u32,
        gpu_name: String,
    ) -> Result<Self, String> {
        let mem_props = unsafe { instance.get_physical_device_memory_properties(phys) };

        {
            let mut st = STATE.lock();
            st.gpu_name = gpu_name;
        }

        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family);
        let cmd_pool = unsafe { device.create_command_pool(&pool_info, None) }
            .map_err(|e| format!("cmd pool: {e}"))?;

        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(cmd_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(FRAMES_IN_FLIGHT as u32);
        let cmds = unsafe { device.allocate_command_buffers(&alloc_info) }
            .map_err(|e| format!("alloc cmds: {e}"))?;

        let v_stride = (MAX_VERTICES_PER_FRAME * size_of::<UiVertex>()) as vk::DeviceSize;
        let i_stride = (MAX_INDICES_PER_FRAME * size_of::<u32>()) as vk::DeviceSize;

        let (vbuf, vmem, vptr) = unsafe {
            create_mapped_buffer(
                &device,
                &mem_props,
                v_stride * FRAMES_IN_FLIGHT as u64,
                vk::BufferUsageFlags::VERTEX_BUFFER,
            )?
        };
        let (ibuf, imem, iptr) = unsafe {
            create_mapped_buffer(
                &device,
                &mem_props,
                i_stride * FRAMES_IN_FLIGHT as u64,
                vk::BufferUsageFlags::INDEX_BUFFER,
            )?
        };
        let (lm_buf, lm_mem, lm_ptr) = unsafe {
            create_mapped_buffer(
                &device,
                &mem_props,
                1024,
                vk::BufferUsageFlags::TRANSFER_SRC,
            )?
        };

        let mut slots = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for (idx, cmd) in cmds.into_iter().enumerate() {
            let fence = unsafe {
                device.create_fence(
                    &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                    None,
                )
            }
            .map_err(|e| format!("fence: {e}"))?;
            let render_finished = unsafe {
                device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            }
            .map_err(|e| format!("sem: {e}"))?;
            slots.push(FrameSlot {
                cmd,
                fence,
                render_finished,
                vb_offset: idx as u64 * v_stride,
                ib_offset: idx as u64 * i_stride,
            });
        }

        // Build and upload font atlas
        let atlas = FontAtlas::build();
        let (atlas_img, atlas_mem, atlas_view, atlas_sampler) =
            unsafe { create_font_atlas(&device, &mem_props, queue_family, &atlas)? };

        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
        let desc_layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
        }
        .map_err(|e| format!("desc layout: {e}"))?;

        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 1,
        }];
        let desc_pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(1)
                    .pool_sizes(&pool_sizes),
                None,
            )
        }
        .map_err(|e| format!("desc pool: {e}"))?;

        let sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(desc_pool)
                    .set_layouts(&[desc_layout]),
            )
        }
        .map_err(|e| format!("alloc desc: {e}"))?;
        let desc_set = sets[0];

        let img_info = [vk::DescriptorImageInfo {
            sampler: atlas_sampler,
            image_view: atlas_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        unsafe {
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(desc_set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&img_info)],
                &[],
            );
        }

        let push_ranges = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            offset: 0,
            size: size_of::<PushConstants>() as u32,
        }];
        let pipe_layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&[desc_layout])
                    .push_constant_ranges(&push_ranges),
                None,
            )
        }
        .map_err(|e| format!("pipe layout: {e}"))?;

        let vert_mod = unsafe { create_shader_module(&device, VERT_SPV)? };
        let frag_mod = unsafe { create_shader_module(&device, FRAG_SPV)? };

        Ok(Self {
            device,
            mem_props,
            queue_family,
            cmd_pool,
            slots,
            frame_cursor: 0,
            vbuf,
            vmem,
            vptr,
            ibuf,
            imem,
            iptr,
            lm_buf,
            lm_mem,
            lm_ptr,
            atlas,
            atlas_img,
            atlas_mem,
            atlas_view,
            atlas_sampler,
            desc_layout,
            desc_pool,
            desc_set,
            pipe_layout,
            vert_mod,
            frag_mod,
            swapchain_res: None,
            draw_list: DrawList::new(1280, 720),
            was_capturing: false,
        })
    }

    pub unsafe fn on_swapchain_created(
        &mut self,
        swapchain: vk::SwapchainKHR,
        format: vk::Format,
        extent: vk::Extent2D,
        images: Vec<vk::Image>,
    ) {
        unsafe { self.destroy_swapchain_resources() };

        let is_srgb = matches!(
            format,
            vk::Format::B8G8R8A8_SRGB
                | vk::Format::R8G8B8A8_SRGB
                | vk::Format::A8B8G8R8_SRGB_PACK32
        );

        let att = [vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let color_ref = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let subpass = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_ref)];
        let Ok(render_pass) = (unsafe {
            self.device.create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&att)
                    .subpasses(&subpass),
                None,
            )
        }) else {
            return;
        };

        let Ok(pipeline) = (unsafe {
            create_overlay_pipeline(
                &self.device,
                self.pipe_layout,
                render_pass,
                self.vert_mod,
                self.frag_mod,
            )
        }) else {
            unsafe { self.device.destroy_render_pass(render_pass, None) };
            return;
        };

        let mut views = Vec::with_capacity(images.len());
        let mut framebuffers = Vec::with_capacity(images.len());

        for &img in &images {
            let view_info = vk::ImageViewCreateInfo::default()
                .image(img)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            let Ok(view) = (unsafe { self.device.create_image_view(&view_info, None) }) else {
                continue;
            };
            let fb_att = [view];
            let fb_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&fb_att)
                .width(extent.width.max(1))
                .height(extent.height.max(1))
                .layers(1);
            let Ok(fb) = (unsafe { self.device.create_framebuffer(&fb_info, None) }) else {
                unsafe { self.device.destroy_image_view(view, None) };
                continue;
            };
            views.push(view);
            framebuffers.push(fb);
        }

        {
            let mut st = STATE.lock();
            st.screen_size = [extent.width.max(1), extent.height.max(1)];
        }

        self.swapchain_res = Some(SwapchainResources {
            swapchain,
            format,
            extent,
            is_srgb,
            images,
            views,
            framebuffers,
            render_pass,
            pipeline,
        });
    }

    pub unsafe fn on_swapchain_destroyed(&mut self, swapchain: vk::SwapchainKHR) {
        if self
            .swapchain_res
            .as_ref()
            .is_some_and(|r| r.swapchain == swapchain)
        {
            unsafe { self.destroy_swapchain_resources() };
        }
    }

    unsafe fn destroy_swapchain_resources(&mut self) {
        if let Some(res) = self.swapchain_res.take() {
            unsafe {
                let _ = self.device.device_wait_idle();
                for fb in res.framebuffers {
                    self.device.destroy_framebuffer(fb, None);
                }
                for v in res.views {
                    self.device.destroy_image_view(v, None);
                }
                self.device.destroy_pipeline(res.pipeline, None);
                self.device.destroy_render_pass(res.render_pass, None);
            }
        }
    }

    /// Renders HUD + Menu + optional instant lightmap update, returning the semaphore to wait on before vkQueuePresentKHR.
    pub unsafe fn render_and_submit(
        &mut self,
        queue: vk::Queue,
        swapchain: vk::SwapchainKHR,
        image_index: u32,
        wait_semaphores: &[vk::Semaphore],
        lightmaps_to_refresh: &[vk::Image],
        normal_lightmap: &[u8; 1024],
    ) -> Option<vk::Semaphore> {
        let res = self.swapchain_res.as_ref()?;
        if res.swapchain != swapchain {
            return None;
        }
        let img = *res.images.get(image_index as usize)?;
        let fb = *res.framebuffers.get(image_index as usize)?;
        let extent = res.extent;
        let is_srgb = res.is_srgb;
        let render_pass = res.render_pass;
        let pipeline = res.pipeline;

        let (refresh_lm, fb_enabled, fb_mode, fb_strength, capturing_now) = {
            let mut st = STATE.lock();
            st.screen_size = [extent.width.max(1), extent.height.max(1)];
            st.record_frame_time();

            let mask = PASSIVE_KEYS_MASK.load(Ordering::Relaxed);
            st.key_w = (mask & 1) != 0;
            st.key_a = (mask & 2) != 0;
            st.key_s = (mask & 4) != 0;
            st.key_d = (mask & 8) != 0;
            st.key_space = (mask & 16) != 0;
            let shift_down = (mask & 128) != 0;
            let now = std::time::Instant::now();
            let dt = now
                .duration_since(st.last_freecam_step)
                .as_secs_f32()
                .min(0.1);
            st.last_freecam_step = now;

            let freecam_on = st.config.freecam_enabled && st.game.in_world;
            set_freecam_active(freecam_on);
            if freecam_on {
                if !st.menu_open && !st.hud_edit_mode {
                    let yr = st.game.yaw.to_radians();
                    let pr = st.game.pitch.to_radians();
                    let fwd = [-yr.sin() * pr.cos(), -pr.sin(), yr.cos() * pr.cos()];
                    let right = [-yr.cos(), 0.0, -yr.sin()];
                    let spd = st.config.freecam_speed * dt;
                    let mut mv = [0.0f32; 3];
                    if st.key_w {
                        for i in 0..3 {
                            mv[i] += fwd[i];
                        }
                    }
                    if st.key_s {
                        for i in 0..3 {
                            mv[i] -= fwd[i];
                        }
                    }
                    if st.key_d {
                        for i in 0..3 {
                            mv[i] += right[i];
                        }
                    }
                    if st.key_a {
                        for i in 0..3 {
                            mv[i] -= right[i];
                        }
                    }
                    if st.key_space {
                        mv[1] += 1.0;
                    }
                    if shift_down {
                        mv[1] -= 1.0;
                    }
                    let mlen = (mv[0] * mv[0] + mv[1] * mv[1] + mv[2] * mv[2]).sqrt();
                    if mlen > 0.01 {
                        for i in 0..3 {
                            st.freecam_offset[i] += (mv[i] / mlen) * spd;
                        }
                    }
                }
            } else {
                st.freecam_offset = [0.0; 3];
            }

            let need_vp = st.config.esp_enabled || st.config.tracers_enabled;
            sync_xray_flags(
                st.config.xray_terrain,
                st.config.xray_entities,
                st.config.xray_ores,
                need_vp,
                freecam_on,
                st.freecam_offset,
            );
            self.draw_list.clear(extent.width, extent.height);
            render_hud(&mut st, &mut self.draw_list, &self.atlas);
            render_menu(&mut st, &mut self.draw_list, &self.atlas);

            st.mouse_just_pressed = false;
            st.mouse_just_released = false;
            st.scroll_delta = 0.0;

            let refresh = st.lightmap_dirty;
            st.lightmap_dirty = false;
            let cap = st.menu_open || st.hud_edit_mode;
            set_ui_capturing(cap);
            (
                refresh,
                st.config.fullbright_enabled,
                st.config.fullbright_mode,
                st.config.fullbright_strength,
                cap,
            )
        };

        if self.was_capturing && !capturing_now {
            sync_cursor_from_ui_transition();
        }
        self.was_capturing = capturing_now;

        let vert_count = self.draw_list.vertices.len().min(MAX_VERTICES_PER_FRAME);
        let idx_count = self.draw_list.indices.len().min(MAX_INDICES_PER_FRAME);
        let has_draws = vert_count > 0 && idx_count > 0 && !self.draw_list.batches.is_empty();
        let has_lm_update = refresh_lm && !lightmaps_to_refresh.is_empty();

        if !has_draws && !has_lm_update {
            return None;
        }

        let slot = &self.slots[self.frame_cursor];
        self.frame_cursor = (self.frame_cursor + 1) % self.slots.len();

        unsafe {
            let _ = self.device.wait_for_fences(&[slot.fence], true, u64::MAX);
            let _ = self.device.reset_fences(&[slot.fence]);
        }

        if has_draws {
            unsafe {
                ptr::copy_nonoverlapping(
                    self.draw_list.vertices.as_ptr() as *const u8,
                    self.vptr.add(slot.vb_offset as usize),
                    vert_count * size_of::<UiVertex>(),
                );
                ptr::copy_nonoverlapping(
                    self.draw_list.indices.as_ptr() as *const u8,
                    self.iptr.add(slot.ib_offset as usize),
                    idx_count * size_of::<u32>(),
                );
            }
        }

        if has_lm_update {
            let mut pixels = *normal_lightmap;
            if fb_enabled {
                match fb_mode {
                    FullbrightMode::Max => pixels.fill(255),
                    FullbrightMode::Soft => {
                        let s = fb_strength.clamp(0.0, 1.0);
                        for (i, px) in pixels.iter_mut().enumerate() {
                            if i % 4 == 3 {
                                *px = 255;
                            } else {
                                let v = *px as f32 + (255.0 - *px as f32) * s;
                                *px = v.round().clamp(0.0, 255.0) as u8;
                            }
                        }
                    }
                }
            }
            unsafe {
                ptr::copy_nonoverlapping(pixels.as_ptr(), self.lm_ptr, 1024);
            }
        }

        let cmd = slot.cmd;
        unsafe {
            let _ = self
                .device
                .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty());
            if self
                .device
                .begin_command_buffer(
                    cmd,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .is_err()
            {
                return None;
            }

            if has_lm_update {
                let range = vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                };
                for &lm_img in lightmaps_to_refresh {
                    let to_dst = vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_READ)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .image(lm_img)
                        .subresource_range(range);
                    self.device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::VERTEX_SHADER
                            | vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[to_dst],
                    );

                    let region = vk::BufferImageCopy {
                        buffer_offset: 0,
                        buffer_row_length: 0,
                        buffer_image_height: 0,
                        image_subresource: vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            mip_level: 0,
                            base_array_layer: 0,
                            layer_count: 1,
                        },
                        image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                        image_extent: vk::Extent3D {
                            width: 16,
                            height: 16,
                            depth: 1,
                        },
                    };
                    self.device.cmd_copy_buffer_to_image(
                        cmd,
                        self.lm_buf,
                        lm_img,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[region],
                    );

                    let to_read = vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .image(lm_img)
                        .subresource_range(range);
                    self.device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::VERTEX_SHADER
                            | vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[to_read],
                    );
                }
            }

            if has_draws {
                let range = vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                };
                let to_color = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::MEMORY_READ)
                    .dst_access_mask(
                        vk::AccessFlags::COLOR_ATTACHMENT_READ
                            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    )
                    .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .image(img)
                    .subresource_range(range);
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_color],
                );

                let rp_begin = vk::RenderPassBeginInfo::default()
                    .render_pass(render_pass)
                    .framebuffer(fb)
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent,
                    });
                self.device
                    .cmd_begin_render_pass(cmd, &rp_begin, vk::SubpassContents::INLINE);
                self.device
                    .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
                self.device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipe_layout,
                    0,
                    &[self.desc_set],
                    &[],
                );

                let vp = vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                };
                self.device.cmd_set_viewport(cmd, 0, &[vp]);

                let pc = PushConstants {
                    screen_size: [extent.width.max(1) as f32, extent.height.max(1) as f32],
                    is_srgb: if is_srgb { 1.0 } else { 0.0 },
                    _pad: 0.0,
                };
                let pc_bytes = std::slice::from_raw_parts(
                    (&pc as *const PushConstants) as *const u8,
                    size_of::<PushConstants>(),
                );
                self.device.cmd_push_constants(
                    cmd,
                    self.pipe_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    pc_bytes,
                );

                self.device
                    .cmd_bind_vertex_buffers(cmd, 0, &[self.vbuf], &[slot.vb_offset]);
                self.device
                    .cmd_bind_index_buffer(cmd, self.ibuf, slot.ib_offset, vk::IndexType::UINT32);

                for batch in &self.draw_list.batches {
                    if batch.index_offset + batch.index_count > idx_count as u32 {
                        continue;
                    }
                    let sc = vk::Rect2D {
                        offset: vk::Offset2D {
                            x: batch.scissor[0].max(0),
                            y: batch.scissor[1].max(0),
                        },
                        extent: vk::Extent2D {
                            width: batch.scissor[2].max(0) as u32,
                            height: batch.scissor[3].max(0) as u32,
                        },
                    };
                    self.device.cmd_set_scissor(cmd, 0, &[sc]);
                    self.device
                        .cmd_draw_indexed(cmd, batch.index_count, 1, batch.index_offset, 0, 0);
                }

                self.device.cmd_end_render_pass(cmd);

                let to_present = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                    .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .image(img)
                    .subresource_range(range);
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_present],
                );
            }

            if self.device.end_command_buffer(cmd).is_err() {
                return None;
            }

            let stage_buf = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT; 8];
            let stage_count = wait_semaphores.len().min(stage_buf.len());
            let sig = [slot.render_finished];
            let cmds = [cmd];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores[..stage_count])
                .wait_dst_stage_mask(&stage_buf[..stage_count])
                .command_buffers(&cmds)
                .signal_semaphores(&sig);
            if self.device.queue_submit(queue, &[submit], slot.fence).is_err() {
                return None;
            }
            Some(slot.render_finished)
        }
    }
}

impl Drop for VulkanOverlay {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            self.destroy_swapchain_resources();
            for s in &self.slots {
                self.device.destroy_fence(s.fence, None);
                self.device.destroy_semaphore(s.render_finished, None);
            }
            self.device.destroy_command_pool(self.cmd_pool, None);
            self.device.destroy_shader_module(self.vert_mod, None);
            self.device.destroy_shader_module(self.frag_mod, None);
            self.device.destroy_pipeline_layout(self.pipe_layout, None);
            self.device.destroy_descriptor_pool(self.desc_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.desc_layout, None);
            self.device.destroy_sampler(self.atlas_sampler, None);
            self.device.destroy_image_view(self.atlas_view, None);
            self.device.destroy_image(self.atlas_img, None);
            self.device.free_memory(self.atlas_mem, None);
            self.device.destroy_buffer(self.vbuf, None);
            self.device.free_memory(self.vmem, None);
            self.device.destroy_buffer(self.ibuf, None);
            self.device.free_memory(self.imem, None);
            self.device.destroy_buffer(self.lm_buf, None);
            self.device.free_memory(self.lm_mem, None);
        }
    }
}

fn find_memory_type(
    props: &vk::PhysicalDeviceMemoryProperties,
    type_bits: u32,
    req: vk::MemoryPropertyFlags,
) -> Option<u32> {
    for i in 0..props.memory_type_count {
        if (type_bits & (1 << i)) != 0
            && props.memory_types[i as usize]
                .property_flags
                .contains(req)
        {
            return Some(i);
        }
    }
    None
}

unsafe fn create_mapped_buffer(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
) -> Result<(vk::Buffer, vk::DeviceMemory, *mut u8), String> {
    let buf = unsafe {
        device.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(size)
                .usage(usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            None,
        )
    }
    .map_err(|e| format!("create_buffer: {e}"))?;
    let req = unsafe { device.get_buffer_memory_requirements(buf) };
    let mem_ty = find_memory_type(
        mem_props,
        req.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )
    .ok_or_else(|| "no host visible memory".to_string())?;

    let mem = unsafe {
        device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(mem_ty),
            None,
        )
    }
    .map_err(|e| format!("alloc_memory: {e}"))?;
    unsafe { device.bind_buffer_memory(buf, mem, 0) }.map_err(|e| format!("bind: {e}"))?;
    let ptr = unsafe { device.map_memory(mem, 0, size, vk::MemoryMapFlags::empty()) }
        .map_err(|e| format!("map: {e}"))? as *mut u8;
    Ok((buf, mem, ptr))
}

unsafe fn create_font_atlas(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    queue_family: u32,
    atlas: &FontAtlas,
) -> Result<(vk::Image, vk::DeviceMemory, vk::ImageView, vk::Sampler), String> {
    let img = unsafe {
        device.create_image(
            &vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .extent(vk::Extent3D {
                    width: atlas.width,
                    height: atlas.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED),
            None,
        )
    }
    .map_err(|e| format!("atlas img: {e}"))?;

    let req = unsafe { device.get_image_memory_requirements(img) };
    let mem_ty = find_memory_type(
        mem_props,
        req.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )
    .ok_or_else(|| "no device local memory".to_string())?;
    let mem = unsafe {
        device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(mem_ty),
            None,
        )
    }
    .map_err(|e| format!("atlas mem: {e}"))?;
    unsafe { device.bind_image_memory(img, mem, 0) }.map_err(|e| format!("atlas bind: {e}"))?;

    // Staging upload using a temporary command pool + queue 0
    let (stg_buf, stg_mem, stg_ptr) = unsafe {
        create_mapped_buffer(
            device,
            mem_props,
            atlas.pixels.len() as vk::DeviceSize,
            vk::BufferUsageFlags::TRANSFER_SRC,
        )?
    };
    unsafe {
        ptr::copy_nonoverlapping(atlas.pixels.as_ptr(), stg_ptr, atlas.pixels.len());
    }

    let pool = unsafe {
        device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(queue_family)
                .flags(vk::CommandPoolCreateFlags::TRANSIENT),
            None,
        )
    }
    .map_err(|e| format!("upload pool: {e}"))?;
    let cmd = unsafe {
        device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )
    }
    .map_err(|e| format!("upload cmd: {e}"))?[0];

    let queue = unsafe { device.get_device_queue(queue_family, 0) };
    let range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    };

    unsafe {
        device
            .begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .map_err(|e| format!("begin: {e}"))?;
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .image(img)
                .subresource_range(range)],
        );
        device.cmd_copy_buffer_to_image(
            cmd,
            stg_buf,
            img,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width: atlas.width,
                    height: atlas.height,
                    depth: 1,
                },
            }],
        );
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(img)
                .subresource_range(range)],
        );
        device.end_command_buffer(cmd).map_err(|e| format!("end: {e}"))?;
        let fence = device
            .create_fence(&vk::FenceCreateInfo::default(), None)
            .map_err(|e| format!("fence: {e}"))?;
        device
            .queue_submit(
                queue,
                &[vk::SubmitInfo::default().command_buffers(&[cmd])],
                fence,
            )
            .map_err(|e| format!("submit: {e}"))?;
        let _ = device.wait_for_fences(&[fence], true, u64::MAX);
        device.destroy_fence(fence, None);
        device.destroy_command_pool(pool, None);
        device.destroy_buffer(stg_buf, None);
        device.free_memory(stg_mem, None);
    }

    let view = unsafe {
        device.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(img)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .subresource_range(range),
            None,
        )
    }
    .map_err(|e| format!("atlas view: {e}"))?;

    let sampler = unsafe {
        device.create_sampler(
            &vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
            None,
        )
    }
    .map_err(|e| format!("atlas sampler: {e}"))?;

    Ok((img, mem, view, sampler))
}

unsafe fn create_shader_module(
    device: &ash::Device,
    bytes: &[u8],
) -> Result<vk::ShaderModule, String> {
    let mut words = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        words.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    unsafe {
        device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
    }
    .map_err(|e| format!("shader module: {e}"))
}

unsafe fn create_overlay_pipeline(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    render_pass: vk::RenderPass,
    vert_mod: vk::ShaderModule,
    frag_mod: vk::ShaderModule,
) -> Result<vk::Pipeline, String> {
    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_mod)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_mod)
            .name(entry),
    ];

    let binding = [vk::VertexInputBindingDescription {
        binding: 0,
        stride: size_of::<UiVertex>() as u32,
        input_rate: vk::VertexInputRate::VERTEX,
    }];
    let attrs = [
        vk::VertexInputAttributeDescription {
            location: 0,
            binding: 0,
            format: vk::Format::R32G32_SFLOAT,
            offset: 0,
        },
        vk::VertexInputAttributeDescription {
            location: 1,
            binding: 0,
            format: vk::Format::R32G32_SFLOAT,
            offset: 8,
        },
        vk::VertexInputAttributeDescription {
            location: 2,
            binding: 0,
            format: vk::Format::R32G32_SFLOAT,
            offset: 16,
        },
        vk::VertexInputAttributeDescription {
            location: 3,
            binding: 0,
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: 24,
        },
        vk::VertexInputAttributeDescription {
            location: 4,
            binding: 0,
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: 40,
        },
        vk::VertexInputAttributeDescription {
            location: 5,
            binding: 0,
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: 56,
        },
    ];
    let vinput = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding)
        .vertex_attribute_descriptions(&attrs);

    let ia = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let vp_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rast = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::CLOCKWISE)
        .line_width(1.0);
    let ms = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let blend_att = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_att);
    let dyn_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dyn_info = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dyn_states);

    let pipe_info = [vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vinput)
        .input_assembly_state(&ia)
        .viewport_state(&vp_state)
        .rasterization_state(&rast)
        .multisample_state(&ms)
        .color_blend_state(&blend)
        .dynamic_state(&dyn_info)
        .layout(layout)
        .render_pass(render_pass)
        .subpass(0)];

    let pipes = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &pipe_info, None) }
        .map_err(|(_, e)| format!("pipeline: {e}"))?;
    Ok(pipes[0])
}
