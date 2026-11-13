use crate::config::FullbrightMode;
use crate::overlay_vk::VulkanOverlay;
use crate::state::STATE;
use ash::vk::{self, Handle};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

type FnGipa = unsafe extern "C" fn(vk::Instance, *const c_char) -> vk::PFN_vkVoidFunction;
type FnGdpa = unsafe extern "C" fn(vk::Device, *const c_char) -> vk::PFN_vkVoidFunction;

static REAL_GIPA: OnceLock<FnGipa> = OnceLock::new();
static REAL_GDPA: OnceLock<FnGdpa> = OnceLock::new();

static FILL_NON_SOLID_ENABLED: AtomicBool = AtomicBool::new(false);

struct InstanceTrack {
    raw: vk::Instance,
    instance: ash::Instance,
    phys_names: HashMap<u64, String>,
}

#[derive(Clone, Copy)]
struct BufferBinding {
    memory: u64,
    offset: u64,
}

struct VkState {
    instances: HashMap<u64, InstanceTrack>,
    phys_to_instance: HashMap<u64, u64>,
    overlay: Option<VulkanOverlay>,
    active_device: u64,
    active_queue_family: u32,
    lightmap_images: Vec<u64>,
    last_normal_lightmap: [u8; 1024],
    mapped_memory: HashMap<u64, *mut u8>,
    buffer_bindings: HashMap<u64, BufferBinding>,
    real_create_instance: Option<vk::PFN_vkCreateInstance>,
    real_destroy_instance: Option<vk::PFN_vkDestroyInstance>,
    real_create_device: Option<vk::PFN_vkCreateDevice>,
    real_destroy_device: Option<vk::PFN_vkDestroyDevice>,
    real_create_swapchain: Option<vk::PFN_vkCreateSwapchainKHR>,
    real_destroy_swapchain: Option<vk::PFN_vkDestroySwapchainKHR>,
    real_queue_present: Option<vk::PFN_vkQueuePresentKHR>,
    real_create_image: Option<vk::PFN_vkCreateImage>,
    real_destroy_image: Option<vk::PFN_vkDestroyImage>,
    real_destroy_buffer: Option<vk::PFN_vkDestroyBuffer>,
    real_bind_buffer_mem: Option<vk::PFN_vkBindBufferMemory>,
    real_map_memory: Option<vk::PFN_vkMapMemory>,
    real_unmap_memory: Option<vk::PFN_vkUnmapMemory>,
    real_free_memory: Option<vk::PFN_vkFreeMemory>,
    real_cmd_copy_b2i: Option<vk::PFN_vkCmdCopyBufferToImage>,
    real_create_graphics_pipelines: Option<vk::PFN_vkCreateGraphicsPipelines>,
    real_destroy_pipeline: Option<vk::PFN_vkDestroyPipeline>,
    real_cmd_bind_pipeline: Option<vk::PFN_vkCmdBindPipeline>,
}

unsafe impl Send for VkState {}

static VK: std::sync::LazyLock<Mutex<VkState>> = std::sync::LazyLock::new(|| {
    Mutex::new(VkState {
        instances: HashMap::new(),
        phys_to_instance: HashMap::new(),
        overlay: None,
        active_device: 0,
        active_queue_family: 0,
        lightmap_images: Vec::new(),
        last_normal_lightmap: [255u8; 1024],
        mapped_memory: HashMap::new(),
        buffer_bindings: HashMap::new(),
        real_create_instance: None,
        real_destroy_instance: None,
        real_create_device: None,
        real_destroy_device: None,
        real_create_swapchain: None,
        real_destroy_swapchain: None,
        real_queue_present: None,
        real_create_image: None,
        real_destroy_image: None,
        real_destroy_buffer: None,
        real_bind_buffer_mem: None,
        real_map_memory: None,
        real_unmap_memory: None,
        real_free_memory: None,
        real_cmd_copy_b2i: None,
        real_create_graphics_pipelines: None,
        real_destroy_pipeline: None,
        real_cmd_bind_pipeline: None,
    })
});

// Lock-free fast-path flags + separate RwLock map for X-Ray pipeline swaps
static XRAY_TERRAIN_MAP: std::sync::LazyLock<RwLock<HashMap<u64, u64>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));
static XRAY_ENTITY_MAP: std::sync::LazyLock<RwLock<HashMap<u64, u64>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn set_real_gipa(p: *mut c_void) {
    if !p.is_null() {
        let f: FnGipa = unsafe { std::mem::transmute(p) };
        let _ = REAL_GIPA.set(f);
    }
}

pub fn set_real_gdpa(p: *mut c_void) {
    if !p.is_null() {
        let f: FnGdpa = unsafe { std::mem::transmute(p) };
        let _ = REAL_GDPA.set(f);
    }
}

fn ensure_real_gipa() -> Option<FnGipa> {
    if let Some(f) = REAL_GIPA.get() {
        return Some(*f);
    }
    unsafe {
        let lib = libc::dlopen(c"libvulkan.so.1".as_ptr(), libc::RTLD_LAZY | libc::RTLD_NOLOAD);
        let lib = if lib.is_null() {
            libc::dlopen(c"libvulkan.so.1".as_ptr(), libc::RTLD_LAZY)
        } else {
            lib
        };
        if !lib.is_null() {
            let p = crate::real_dlsym(lib, c"vkGetInstanceProcAddr".as_ptr());
            if !p.is_null() {
                let f: FnGipa = std::mem::transmute(p);
                let _ = REAL_GIPA.set(f);
                return Some(f);
            }
        }
    }
    None
}

unsafe fn fn_ptr<T>(f: T) -> vk::PFN_vkVoidFunction {
    unsafe { std::mem::transmute_copy(&f) }
}

pub unsafe extern "C" fn hook_vkGetInstanceProcAddr(
    instance: vk::Instance,
    p_name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    let real_gipa = ensure_real_gipa();
    let real_fn = real_gipa.and_then(|f| unsafe { f(instance, p_name) });
    if p_name.is_null() {
        return real_fn;
    }
    let name = unsafe { CStr::from_ptr(p_name) }.to_bytes();
    match name {
        b"vkGetInstanceProcAddr" => return unsafe { fn_ptr(hook_vkGetInstanceProcAddr as FnGipa) },
        b"vkGetDeviceProcAddr" => {
            if let Some(rf) = real_fn {
                set_real_gdpa(rf as *mut c_void);
            }
            return unsafe { fn_ptr(hook_vkGetDeviceProcAddr as FnGdpa) };
        }
        b"vkCreateInstance" => {
            if let Some(rf) = real_fn {
                VK.lock().real_create_instance = Some(unsafe { std::mem::transmute(rf) });
                return unsafe { fn_ptr(hook_vkCreateInstance as vk::PFN_vkCreateInstance) };
            }
        }
        b"vkDestroyInstance" => {
            if let Some(rf) = real_fn {
                VK.lock().real_destroy_instance = Some(unsafe { std::mem::transmute(rf) });
                return unsafe { fn_ptr(hook_vkDestroyInstance as vk::PFN_vkDestroyInstance) };
            }
        }
        b"vkCreateDevice" => {
            if let Some(rf) = real_fn {
                VK.lock().real_create_device = Some(unsafe { std::mem::transmute(rf) });
                return unsafe { fn_ptr(hook_vkCreateDevice as vk::PFN_vkCreateDevice) };
            }
        }
        _ => {}
    }

    if let Some(intercepted) = maybe_intercept_device_fn(name, real_fn) {
        return Some(intercepted);
    }
    real_fn
}

pub unsafe extern "C" fn hook_vkGetDeviceProcAddr(
    device: vk::Device,
    p_name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    let real_fn = REAL_GDPA.get().and_then(|f| unsafe { f(device, p_name) });
    if p_name.is_null() || real_fn.is_none() {
        return real_fn;
    }
    let name = unsafe { CStr::from_ptr(p_name) }.to_bytes();
    if name == b"vkGetDeviceProcAddr" {
        return unsafe { fn_ptr(hook_vkGetDeviceProcAddr as FnGdpa) };
    }
    if let Some(intercepted) = maybe_intercept_device_fn(name, real_fn) {
        return Some(intercepted);
    }
    real_fn
}

fn maybe_intercept_device_fn(
    name: &[u8],
    real_fn: vk::PFN_vkVoidFunction,
) -> vk::PFN_vkVoidFunction {
    let rf = real_fn?;
    let mut vk_st = VK.lock();
    unsafe {
        match name {
            b"vkDestroyDevice" => {
                vk_st.real_destroy_device = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkDestroyDevice as vk::PFN_vkDestroyDevice)
            }
            b"vkCreateSwapchainKHR" => {
                vk_st.real_create_swapchain = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkCreateSwapchainKHR as vk::PFN_vkCreateSwapchainKHR)
            }
            b"vkDestroySwapchainKHR" => {
                vk_st.real_destroy_swapchain = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkDestroySwapchainKHR as vk::PFN_vkDestroySwapchainKHR)
            }
            b"vkQueuePresentKHR" => {
                vk_st.real_queue_present = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkQueuePresentKHR as vk::PFN_vkQueuePresentKHR)
            }
            b"vkCreateImage" => {
                vk_st.real_create_image = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkCreateImage as vk::PFN_vkCreateImage)
            }
            b"vkDestroyImage" => {
                vk_st.real_destroy_image = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkDestroyImage as vk::PFN_vkDestroyImage)
            }
            b"vkDestroyBuffer" => {
                vk_st.real_destroy_buffer = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkDestroyBuffer as vk::PFN_vkDestroyBuffer)
            }
            b"vkBindBufferMemory" => {
                vk_st.real_bind_buffer_mem = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkBindBufferMemory as vk::PFN_vkBindBufferMemory)
            }
            b"vkMapMemory" => {
                vk_st.real_map_memory = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkMapMemory as vk::PFN_vkMapMemory)
            }
            b"vkUnmapMemory" => {
                vk_st.real_unmap_memory = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkUnmapMemory as vk::PFN_vkUnmapMemory)
            }
            b"vkFreeMemory" => {
                vk_st.real_free_memory = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkFreeMemory as vk::PFN_vkFreeMemory)
            }
            b"vkCmdCopyBufferToImage" => {
                vk_st.real_cmd_copy_b2i = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkCmdCopyBufferToImage as vk::PFN_vkCmdCopyBufferToImage)
            }
            b"vkCreateGraphicsPipelines" => {
                vk_st.real_create_graphics_pipelines = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkCreateGraphicsPipelines as vk::PFN_vkCreateGraphicsPipelines)
            }
            b"vkDestroyPipeline" => {
                vk_st.real_destroy_pipeline = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkDestroyPipeline as vk::PFN_vkDestroyPipeline)
            }
            b"vkCmdBindPipeline" => {
                vk_st.real_cmd_bind_pipeline = Some(std::mem::transmute(rf));
                fn_ptr(hook_vkCmdBindPipeline as vk::PFN_vkCmdBindPipeline)
            }
            _ => return None,
        }
    }
}

unsafe extern "system" fn hook_vkCreateInstance(
    p_create_info: *const vk::InstanceCreateInfo<'_>,
    p_allocator: *const vk::AllocationCallbacks<'_>,
    p_instance: *mut vk::Instance,
) -> vk::Result {
    let real = VK.lock().real_create_instance.expect("real_create_instance");
    let res = unsafe { real(p_create_info, p_allocator, p_instance) };
    if res == vk::Result::SUCCESS && !p_instance.is_null() {
        let inst = unsafe { *p_instance };
        if let Some(gipa) = ensure_real_gipa() {
            let ash_inst = unsafe {
                ash::Instance::load_with(
                    |name| {
                        gipa(inst, name.as_ptr())
                            .map(|f| std::mem::transmute::<_, *const c_void>(f))
                            .unwrap_or(ptr::null())
                    },
                    inst,
                )
            };
            let mut vk_st = VK.lock();
            let mut phys_names = HashMap::new();
            if let Ok(gpus) = unsafe { ash_inst.enumerate_physical_devices() } {
                for pd in gpus {
                    let props = unsafe { ash_inst.get_physical_device_properties(pd) };
                    let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) }
                        .to_string_lossy()
                        .to_string();
                    phys_names.insert(pd.as_raw(), name);
                    vk_st.phys_to_instance.insert(pd.as_raw(), inst.as_raw());
                }
            }
            vk_st.instances.insert(
                inst.as_raw(),
                InstanceTrack {
                    raw: inst,
                    instance: ash_inst,
                    phys_names,
                },
            );
        }
    }
    res
}

unsafe extern "system" fn hook_vkDestroyInstance(
    instance: vk::Instance,
    p_allocator: *const vk::AllocationCallbacks<'_>,
) {
    let real = {
        let mut vk_st = VK.lock();
        vk_st.instances.remove(&instance.as_raw());
        vk_st.real_destroy_instance
    };
    if let Some(f) = real {
        unsafe { f(instance, p_allocator) };
    }
}

unsafe extern "system" fn hook_vkCreateDevice(
    physical_device: vk::PhysicalDevice,
    p_create_info: *const vk::DeviceCreateInfo<'_>,
    p_allocator: *const vk::AllocationCallbacks<'_>,
    p_device: *mut vk::Device,
) -> vk::Result {
    let (real, inst_opt, gpu_name) = {
        let vk_st = VK.lock();
        let inst_raw = vk_st.phys_to_instance.get(&physical_device.as_raw()).copied();
        let track = inst_raw.and_then(|id| vk_st.instances.get(&id));
        let name = track
            .and_then(|t| t.phys_names.get(&physical_device.as_raw()))
            .cloned()
            .unwrap_or_else(|| "Vulkan GPU".to_string());
        (vk_st.real_create_device, track.map(|t| t.instance.clone()), name)
    };
    let Some(real) = real else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };

    // Enable fillModeNonSolid if supported so Terrain Cave Wireframe X-Ray works cleanly!
    let mut patched_ci = unsafe { *p_create_info };
    let mut patched_features = vk::PhysicalDeviceFeatures::default();
    let mut patched_features2_copy: Option<vk::PhysicalDeviceFeatures2<'_>> = None;
    let mut want_wireframe = false;

    if let Some(inst) = &inst_opt {
        let supported = unsafe { inst.get_physical_device_features(physical_device) };
        if supported.fill_mode_non_solid == vk::TRUE {
            want_wireframe = true;
            if !patched_ci.p_enabled_features.is_null() {
                patched_features = unsafe { *patched_ci.p_enabled_features };
                patched_features.fill_mode_non_solid = vk::TRUE;
                patched_ci.p_enabled_features = &patched_features;
            } else {
                // Check pNext for VkPhysicalDeviceFeatures2
                let mut cur = patched_ci.p_next as *const vk::BaseInStructure<'_>;
                let mut found_f2 = false;
                while !cur.is_null() {
                    let s = unsafe { &*cur };
                    if s.s_type == vk::StructureType::PHYSICAL_DEVICE_FEATURES_2 {
                        let f2 = unsafe { &*(cur as *const vk::PhysicalDeviceFeatures2<'_>) };
                        let mut copy = *f2;
                        copy.features.fill_mode_non_solid = vk::TRUE;
                        patched_features2_copy = Some(copy);
                        found_f2 = true;
                        break;
                    }
                    cur = s.p_next;
                }
                if found_f2 {
                    if let Some(f2_mut) = &mut patched_features2_copy {
                        // Safe shallow splice only when pNext is Features2 at head or fallback to pEnabledFeatures if not head
                        let head = patched_ci.p_next as *const vk::BaseInStructure<'_>;
                        if !head.is_null()
                            && unsafe { (*head).s_type }
                                == vk::StructureType::PHYSICAL_DEVICE_FEATURES_2
                        {
                            patched_ci.p_next = (f2_mut as *mut vk::PhysicalDeviceFeatures2<'_>)
                                as *const c_void;
                        } else {
                            // Mutate in place only if not at head; wgpu always puts Features2 or pEnabledFeatures
                            let ptr_mut = head as *mut vk::BaseInStructure<'_>;
                            let mut c = ptr_mut;
                            while !c.is_null() {
                                if unsafe { (*c).s_type }
                                    == vk::StructureType::PHYSICAL_DEVICE_FEATURES_2
                                {
                                    let f2m = unsafe {
                                        &mut *(c as *mut vk::PhysicalDeviceFeatures2<'_>)
                                    };
                                    f2m.features.fill_mode_non_solid = vk::TRUE;
                                    break;
                                }
                                c = unsafe { (*c).p_next as *mut vk::BaseInStructure<'_> };
                            }
                        }
                    }
                } else {
                    patched_features.fill_mode_non_solid = vk::TRUE;
                    patched_ci.p_enabled_features = &patched_features;
                }
            }
        }
    }

    let res = unsafe { real(physical_device, &patched_ci, p_allocator, p_device) };
    if res == vk::Result::SUCCESS && !p_device.is_null() {
        FILL_NON_SOLID_ENABLED.store(want_wireframe, Ordering::Relaxed);
        let dev = unsafe { *p_device };
        let queue_family = if patched_ci.queue_create_info_count > 0
            && !patched_ci.p_queue_create_infos.is_null()
        {
            unsafe { (*patched_ci.p_queue_create_infos).queue_family_index }
        } else {
            0
        };

        if let (Some(inst), Some(gdpa)) = (inst_opt, REAL_GDPA.get().copied()) {
            let ash_dev = unsafe {
                ash::Device::load_with(
                    |name| {
                        gdpa(dev, name.as_ptr())
                            .map(|f| std::mem::transmute::<_, *const c_void>(f))
                            .unwrap_or(ptr::null())
                    },
                    dev,
                )
            };
            match unsafe { VulkanOverlay::new(&inst, physical_device, ash_dev, queue_family, gpu_name) }
            {
                Ok(ov) => {
                    let mut vk_st = VK.lock();
                    vk_st.overlay = Some(ov);
                    vk_st.active_device = dev.as_raw();
                    vk_st.active_queue_family = queue_family;
                }
                Err(_) => {}
            }
        }
    }
    res
}

unsafe extern "system" fn hook_vkDestroyDevice(
    device: vk::Device,
    p_allocator: *const vk::AllocationCallbacks<'_>,
) {
    let real = {
        let mut vk_st = VK.lock();
        if vk_st.active_device == device.as_raw() {
            vk_st.overlay = None;
            vk_st.active_device = 0;
            vk_st.lightmap_images.clear();
            XRAY_TERRAIN_MAP.write().clear();
            XRAY_ENTITY_MAP.write().clear();
        }
        vk_st.real_destroy_device
    };
    if let Some(f) = real {
        unsafe { f(device, p_allocator) };
    }
}

unsafe extern "system" fn hook_vkCreateSwapchainKHR(
    device: vk::Device,
    p_create_info: *const vk::SwapchainCreateInfoKHR<'_>,
    p_allocator: *const vk::AllocationCallbacks<'_>,
    p_swapchain: *mut vk::SwapchainKHR,
) -> vk::Result {
    let real = VK.lock().real_create_swapchain.expect("real_create_swapchain");
    let res = unsafe { real(device, p_create_info, p_allocator, p_swapchain) };
    if res == vk::Result::SUCCESS && !p_swapchain.is_null() && !p_create_info.is_null() {
        let sw = unsafe { *p_swapchain };
        let ci = unsafe { &*p_create_info };
        if let Some(gdpa) = REAL_GDPA.get().copied()
            && let Some(p_get_imgs) = unsafe { gdpa(device, c"vkGetSwapchainImagesKHR".as_ptr()) }
        {
            let get_imgs: vk::PFN_vkGetSwapchainImagesKHR = unsafe { std::mem::transmute(p_get_imgs) };
            let mut count = 0u32;
            let _ = unsafe { get_imgs(device, sw, &mut count, ptr::null_mut()) };
            if count > 0 {
                let mut imgs = vec![vk::Image::null(); count as usize];
                if unsafe { get_imgs(device, sw, &mut count, imgs.as_mut_ptr()) }
                    == vk::Result::SUCCESS
                {
                    let mut vk_st = VK.lock();
                    if let Some(ov) = vk_st.overlay.as_mut() {
                        unsafe {
                            ov.on_swapchain_created(sw, ci.image_format, ci.image_extent, imgs);
                        }
                    }
                }
            }
        }
    }
    res
}

unsafe extern "system" fn hook_vkDestroySwapchainKHR(
    device: vk::Device,
    swapchain: vk::SwapchainKHR,
    p_allocator: *const vk::AllocationCallbacks<'_>,
) {
    let real = {
        let mut vk_st = VK.lock();
        if let Some(ov) = vk_st.overlay.as_mut() {
            unsafe { ov.on_swapchain_destroyed(swapchain) };
        }
        vk_st.real_destroy_swapchain
    };
    if let Some(f) = real {
        unsafe { f(device, swapchain, p_allocator) };
    }
}

unsafe extern "system" fn hook_vkQueuePresentKHR(
    queue: vk::Queue,
    p_present_info: *const vk::PresentInfoKHR<'_>,
) -> vk::Result {
    let mut modified_present;
    let mut new_wait = [vk::Semaphore::null(); 1];
    let present_ptr: *const vk::PresentInfoKHR<'_>;

    let real_present = {
        let mut vk_st = VK.lock();
        let real = vk_st.real_queue_present;
        let pi = unsafe { &*p_present_info };
        if pi.swapchain_count > 0
            && !pi.p_swapchains.is_null()
            && !pi.p_image_indices.is_null()
        {
            let sw = unsafe { *pi.p_swapchains };
            let idx = unsafe { *pi.p_image_indices };
            let waits = if pi.wait_semaphore_count > 0 && !pi.p_wait_semaphores.is_null() {
                unsafe {
                    std::slice::from_raw_parts(
                        pi.p_wait_semaphores,
                        pi.wait_semaphore_count as usize,
                    )
                }
            } else {
                &[]
            };
            let lm_imgs = vk_st.lightmap_images.clone();
            let normal_lm = vk_st.last_normal_lightmap;
            let lm_handles: Vec<vk::Image> =
                lm_imgs.into_iter().map(vk::Image::from_raw).collect();

            if let Some(ov) = vk_st.overlay.as_mut() {
                if let Some(sig) = unsafe {
                    ov.render_and_submit(queue, sw, idx, waits, &lm_handles, &normal_lm)
                } {
                    new_wait[0] = sig;
                    modified_present = *pi;
                    modified_present.wait_semaphore_count = 1;
                    modified_present.p_wait_semaphores = new_wait.as_ptr();
                    present_ptr = &modified_present;
                } else {
                    present_ptr = p_present_info;
                }
            } else {
                present_ptr = p_present_info;
            }
        } else {
            present_ptr = p_present_info;
        }
        real
    };

    if let Some(f) = real_present {
        unsafe { f(queue, present_ptr) }
    } else {
        vk::Result::ERROR_DEVICE_LOST
    }
}

// ---------------------------------------------------------
// Lightmap Interception (Fullbright)
// ---------------------------------------------------------

unsafe extern "system" fn hook_vkCreateImage(
    device: vk::Device,
    p_create_info: *const vk::ImageCreateInfo<'_>,
    p_allocator: *const vk::AllocationCallbacks<'_>,
    p_image: *mut vk::Image,
) -> vk::Result {
    let real = VK.lock().real_create_image.expect("real_create_image");
    let res = unsafe { real(device, p_create_info, p_allocator, p_image) };
    if res == vk::Result::SUCCESS && !p_create_info.is_null() && !p_image.is_null() {
        let ci = unsafe { &*p_create_info };
        if ci.image_type == vk::ImageType::TYPE_2D
            && ci.extent.width == 16
            && ci.extent.height == 16
            && ci.extent.depth == 1
            && ci.mip_levels == 1
            && ci.array_layers == 1
            && ci.samples == vk::SampleCountFlags::TYPE_1
            && ci.format == vk::Format::R8G8B8A8_UNORM
            && ci.usage.contains(vk::ImageUsageFlags::TRANSFER_DST)
            && ci.usage.contains(vk::ImageUsageFlags::SAMPLED)
        {
            let img_raw = unsafe { *p_image }.as_raw();
            let mut vk_st = VK.lock();
            vk_st.lightmap_images.push(img_raw);
            let count = vk_st.lightmap_images.len();
            drop(vk_st);

            let mut st = STATE.lock();
            st.lightmap_hooked = true;
            st.lightmap_images_seen = count;
        }
    }
    res
}

unsafe extern "system" fn hook_vkDestroyImage(
    device: vk::Device,
    image: vk::Image,
    p_allocator: *const vk::AllocationCallbacks<'_>,
) {
    let real = {
        let mut vk_st = VK.lock();
        vk_st.lightmap_images.retain(|&x| x != image.as_raw());
        vk_st.real_destroy_image
    };
    if let Some(f) = real {
        unsafe { f(device, image, p_allocator) };
    }
}

unsafe extern "system" fn hook_vkDestroyBuffer(
    device: vk::Device,
    buffer: vk::Buffer,
    p_allocator: *const vk::AllocationCallbacks<'_>,
) {
    let real = {
        let mut vk_st = VK.lock();
        vk_st.buffer_bindings.remove(&buffer.as_raw());
        vk_st.real_destroy_buffer
    };
    if let Some(f) = real {
        unsafe { f(device, buffer, p_allocator) };
    }
}

unsafe extern "system" fn hook_vkBindBufferMemory(
    device: vk::Device,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    memory_offset: vk::DeviceSize,
) -> vk::Result {
    let real = VK.lock().real_bind_buffer_mem.expect("real_bind_buffer_mem");
    let res = unsafe { real(device, buffer, memory, memory_offset) };
    if res == vk::Result::SUCCESS {
        VK.lock().buffer_bindings.insert(
            buffer.as_raw(),
            BufferBinding {
                memory: memory.as_raw(),
                offset: memory_offset,
            },
        );
    }
    res
}

unsafe extern "system" fn hook_vkMapMemory(
    device: vk::Device,
    memory: vk::DeviceMemory,
    offset: vk::DeviceSize,
    size: vk::DeviceSize,
    flags: vk::MemoryMapFlags,
    pp_data: *mut *mut c_void,
) -> vk::Result {
    let real = VK.lock().real_map_memory.expect("real_map_memory");
    let res = unsafe { real(device, memory, offset, size, flags, pp_data) };
    if res == vk::Result::SUCCESS && !pp_data.is_null() && !unsafe { *pp_data }.is_null() {
        // Store base pointer adjusted by -offset so (base + bind_offset + bufferOffset) stays exact
        let base_ptr = unsafe { (*pp_data as *mut u8).offset(-(offset as isize)) };
        VK.lock().mapped_memory.insert(memory.as_raw(), base_ptr);
    }
    res
}

unsafe extern "system" fn hook_vkUnmapMemory(device: vk::Device, memory: vk::DeviceMemory) {
    let real = {
        let mut vk_st = VK.lock();
        vk_st.mapped_memory.remove(&memory.as_raw());
        vk_st.real_unmap_memory
    };
    if let Some(f) = real {
        unsafe { f(device, memory) };
    }
}

unsafe extern "system" fn hook_vkFreeMemory(
    device: vk::Device,
    memory: vk::DeviceMemory,
    p_allocator: *const vk::AllocationCallbacks<'_>,
) {
    let real = {
        let mut vk_st = VK.lock();
        vk_st.mapped_memory.remove(&memory.as_raw());
        vk_st.real_free_memory
    };
    if let Some(f) = real {
        unsafe { f(device, memory, p_allocator) };
    }
}

unsafe extern "system" fn hook_vkCmdCopyBufferToImage(
    command_buffer: vk::CommandBuffer,
    src_buffer: vk::Buffer,
    dst_image: vk::Image,
    dst_image_layout: vk::ImageLayout,
    region_count: u32,
    p_regions: *const vk::BufferImageCopy,
) {
    let real = {
        let mut vk_st = VK.lock();
        if region_count > 0
            && !p_regions.is_null()
            && vk_st.lightmap_images.contains(&dst_image.as_raw())
        {
            let region = unsafe { &*p_regions };
            if region.image_extent.width == 16
                && region.image_extent.height == 16
                && region.image_offset.x == 0
                && region.image_offset.y == 0
                && let Some(binding) = vk_st.buffer_bindings.get(&src_buffer.as_raw()).copied()
                && let Some(&base_ptr) = vk_st.mapped_memory.get(&binding.memory)
            {
                let row_texels = if region.buffer_row_length > 0 {
                    region.buffer_row_length as usize
                } else {
                    16
                };
                let row_stride = row_texels * 4;
                let src_base = unsafe {
                    base_ptr.add(binding.offset as usize + region.buffer_offset as usize)
                };

                // Save original 16x16 lightmap bytes so disabling Fullbright restores exact world lighting
                for r in 0..16usize {
                    unsafe {
                        ptr::copy_nonoverlapping(
                            src_base.add(r * row_stride),
                            vk_st.last_normal_lightmap.as_mut_ptr().add(r * 64),
                            64,
                        );
                    }
                }

                let (fb_on, fb_mode, fb_strength) = {
                    let st = STATE.lock();
                    (
                        st.config.fullbright_enabled,
                        st.config.fullbright_mode,
                        st.config.fullbright_strength,
                    )
                };

                if fb_on {
                    for r in 0..16usize {
                        let dst_row =
                            unsafe { std::slice::from_raw_parts_mut(src_base.add(r * row_stride), 64) };
                        match fb_mode {
                            FullbrightMode::Max => dst_row.fill(255),
                            FullbrightMode::Soft => {
                                let s = fb_strength.clamp(0.0, 1.0);
                                for (i, px) in dst_row.iter_mut().enumerate() {
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
                }
            }
        }
        vk_st.real_cmd_copy_b2i
    };

    if let Some(f) = real {
        unsafe {
            f(
                command_buffer,
                src_buffer,
                dst_image,
                dst_image_layout,
                region_count,
                p_regions,
            );
        }
    }
}

// ---------------------------------------------------------
// X-Ray Pipeline Cloning & Swapping (Cave Wireframe + Entity/Chest ESP)
// ---------------------------------------------------------

fn classify_pipeline(ci: &vk::GraphicsPipelineCreateInfo<'_>) -> (bool, bool) {
    if ci.stage_count == 0 || ci.p_stages.is_null() {
        return (false, false);
    }
    let stages = unsafe { std::slice::from_raw_parts(ci.p_stages, ci.stage_count as usize) };
    let mut has_terrain_vs = false;
    let mut has_terrain_fs = false;
    let mut has_entity_vs = false;
    let mut has_entity_fs = false;

    for st in stages {
        if st.p_name.is_null() {
            continue;
        }
        let name = unsafe { CStr::from_ptr(st.p_name) }.to_string_lossy();
        match name.as_ref() {
            "vs_terrain"
            | "vs_terrain_opaque"
            | "vs_terrain_cutout"
            | "vs_section"
            | "vs_section_opaque"
            | "vs_section_cutout"
            | "vs_section_flat"
            | "vs_section_flat_opaque"
            | "vs_section_flat_cutout" => has_terrain_vs = true,
            "fs_solid" | "fs_opaque" => has_terrain_fs = true,
            "vs_entity" | "vs_static_entity" => has_entity_vs = true,
            "fs_entity" => has_entity_fs = true,
            _ => {}
        }
    }
    (
        has_terrain_vs && has_terrain_fs,
        has_entity_vs && has_entity_fs,
    )
}

unsafe extern "system" fn hook_vkCreateGraphicsPipelines(
    device: vk::Device,
    pipeline_cache: vk::PipelineCache,
    create_info_count: u32,
    p_create_infos: *const vk::GraphicsPipelineCreateInfo<'_>,
    p_allocator: *const vk::AllocationCallbacks<'_>,
    p_pipelines: *mut vk::Pipeline,
) -> vk::Result {
    let real = VK
        .lock()
        .real_create_graphics_pipelines
        .expect("real_create_graphics_pipelines");
    let res = unsafe {
        real(
            device,
            pipeline_cache,
            create_info_count,
            p_create_infos,
            p_allocator,
            p_pipelines,
        )
    };
    if res != vk::Result::SUCCESS || p_create_infos.is_null() || p_pipelines.is_null() {
        return res;
    }

    let infos = unsafe { std::slice::from_raw_parts(p_create_infos, create_info_count as usize) };
    let out_pipes = unsafe { std::slice::from_raw_parts(p_pipelines, create_info_count as usize) };
    let wireframe_ok = FILL_NON_SOLID_ENABLED.load(Ordering::Relaxed);

    for (ci, &orig_pipe) in infos.iter().zip(out_pipes.iter()) {
        if orig_pipe == vk::Pipeline::null() {
            continue;
        }
        let (is_terrain, is_entity) = classify_pipeline(ci);
        if is_terrain && wireframe_ok && !ci.p_rasterization_state.is_null() {
            let mut rast = unsafe { *ci.p_rasterization_state };
            rast.polygon_mode = vk::PolygonMode::LINE;
            rast.cull_mode = vk::CullModeFlags::NONE;
            rast.line_width = 1.0;
            let mut clone_ci = *ci;
            clone_ci.p_rasterization_state = &rast;
            let mut twin = vk::Pipeline::null();
            if unsafe { real(device, pipeline_cache, 1, &clone_ci, p_allocator, &mut twin) }
                == vk::Result::SUCCESS
                && twin != vk::Pipeline::null()
            {
                XRAY_TERRAIN_MAP
                    .write()
                    .insert(orig_pipe.as_raw(), twin.as_raw());
            }
        } else if is_entity && !ci.p_depth_stencil_state.is_null() {
            let mut ds = unsafe { *ci.p_depth_stencil_state };
            ds.depth_test_enable = vk::FALSE;
            ds.depth_write_enable = vk::FALSE;
            ds.depth_compare_op = vk::CompareOp::ALWAYS;
            let mut clone_ci = *ci;
            clone_ci.p_depth_stencil_state = &ds;
            let mut twin = vk::Pipeline::null();
            if unsafe { real(device, pipeline_cache, 1, &clone_ci, p_allocator, &mut twin) }
                == vk::Result::SUCCESS
                && twin != vk::Pipeline::null()
            {
                XRAY_ENTITY_MAP
                    .write()
                    .insert(orig_pipe.as_raw(), twin.as_raw());
            }
        }
    }
    res
}

unsafe extern "system" fn hook_vkDestroyPipeline(
    device: vk::Device,
    pipeline: vk::Pipeline,
    p_allocator: *const vk::AllocationCallbacks<'_>,
) {
    let real = VK.lock().real_destroy_pipeline;
    let raw = pipeline.as_raw();
    let twin_t = XRAY_TERRAIN_MAP.write().remove(&raw);
    let twin_e = XRAY_ENTITY_MAP.write().remove(&raw);
    if let Some(f) = real {
        if let Some(t) = twin_t {
            unsafe { f(device, vk::Pipeline::from_raw(t), p_allocator) };
        }
        if let Some(e) = twin_e {
            unsafe { f(device, vk::Pipeline::from_raw(e), p_allocator) };
        }
        unsafe { f(device, pipeline, p_allocator) };
    }
}

unsafe extern "system" fn hook_vkCmdBindPipeline(
    command_buffer: vk::CommandBuffer,
    pipeline_bind_point: vk::PipelineBindPoint,
    mut pipeline: vk::Pipeline,
) {
    if pipeline_bind_point == vk::PipelineBindPoint::GRAPHICS {
        let (xt, xe) = {
            let st = STATE.lock();
            (st.config.xray_terrain, st.config.xray_entities)
        };
        let raw = pipeline.as_raw();
        if xt && let Some(&twin) = XRAY_TERRAIN_MAP.read().get(&raw) {
            pipeline = vk::Pipeline::from_raw(twin);
        } else if xe && let Some(&twin) = XRAY_ENTITY_MAP.read().get(&raw) {
            pipeline = vk::Pipeline::from_raw(twin);
        }
    }
    if let Some(real) = VK.lock().real_cmd_bind_pipeline {
        unsafe { real(command_buffer, pipeline_bind_point, pipeline) };
    }
}
