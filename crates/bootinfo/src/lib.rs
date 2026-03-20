#![no_std]

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Unknown = 0,
    Rgb = 1,
    Bgr = 2,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct FramebufferInfo {
    pub base: *mut u8,
    pub size: usize,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub format: PixelFormat,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct BootInfo {
    pub framebuffer: FramebufferInfo,

    pub memory_map_ptr: *const u8,
    pub memory_map_len: usize,
    pub memory_map_desc_size: usize,
    pub memory_map_desc_version: u32,
    
    pub physical_memory_offset: u64,
}
