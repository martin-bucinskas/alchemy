use alchemy_bootinfo::{FramebufferInfo, PixelFormat};
use uefi::boot;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat as GopPixelFormat};

pub fn probe_framebuffer() -> FramebufferInfo {
  let handle =
    boot::get_handle_for_protocol::<GraphicsOutput>().expect("no GOP handle found");
  let mut gop =
    boot::open_protocol_exclusive::<GraphicsOutput>(handle).expect("failed to open GOP");

  let mode = gop.current_mode_info();
  let (width, height) = mode.resolution();
  let stride = mode.stride();

  let format = match mode.pixel_format() {
    GopPixelFormat::Rgb => PixelFormat::Rgb,
    GopPixelFormat::Bgr => PixelFormat::Bgr,
    _ => PixelFormat::Unknown,
  };

  let mut fb = gop.frame_buffer();

  FramebufferInfo {
    base: fb.as_mut_ptr(),
    size: fb.size(),
    width,
    height,
    stride,
    format,
  }
}

pub fn draw_loader_test_pattern(info: FramebufferInfo) {
  let bytes_per_pixel = 4;

  for y in 0..info.height {
    for x in 0..info.width {
      let pixel_offset = y * info.stride + x;
      let pixel_ptr = unsafe { info.base.add(pixel_offset * bytes_per_pixel) };

      unsafe {
        match info.format {
          PixelFormat::Rgb => {
            *pixel_ptr.add(0) = 0;
            *pixel_ptr.add(1) = 0;
            *pixel_ptr.add(2) = 0;
            *pixel_ptr.add(3) = 0;
          }
          PixelFormat::Bgr => {
            *pixel_ptr.add(0) = 0;
            *pixel_ptr.add(1) = 0;
            *pixel_ptr.add(2) = 0;
            *pixel_ptr.add(3) = 0;
          }
          PixelFormat::Unknown => {}
        }
      }
    }
  }

  for y in 40..160 {
    for x in 40..260 {
      let pixel_offset = y * info.stride + x;
      let pixel_ptr = unsafe { info.base.add(pixel_offset * bytes_per_pixel) };

      unsafe {
        match info.format {
          PixelFormat::Rgb => {
            *pixel_ptr.add(0) = 180;
            *pixel_ptr.add(1) = 40;
            *pixel_ptr.add(2) = 40;
            *pixel_ptr.add(3) = 0;
          }
          PixelFormat::Bgr => {
            *pixel_ptr.add(0) = 40;
            *pixel_ptr.add(1) = 40;
            *pixel_ptr.add(2) = 180;
            *pixel_ptr.add(3) = 0;
          }
          PixelFormat::Unknown => {}
        }
      }
    }
  }
}

pub fn clear_framebuffer(info: FramebufferInfo, r: u8, g: u8, b: u8) {
  for y in 0..info.height {
    for x in 0..info.width {
      let pixel_index = y * info.stride + x;
      let pixel_ptr = unsafe { info.base.add(pixel_index * 4) };

      unsafe {
        match info.format {
          PixelFormat::Rgb => {
            *pixel_ptr.add(0) = r;
            *pixel_ptr.add(1) = g;
            *pixel_ptr.add(2) = b;
            *pixel_ptr.add(3) = 0;
          }
          PixelFormat::Bgr => {
            *pixel_ptr.add(0) = b;
            *pixel_ptr.add(1) = g;
            *pixel_ptr.add(2) = r;
            *pixel_ptr.add(3) = 0;
          }
          PixelFormat::Unknown => {}
        }
      }
    }
  }
}

pub fn fill_rect(info: FramebufferInfo, x0: usize, y0: usize, w: usize, h: usize, r: u8, g: u8, b: u8) {
  let bytes_per_pixel = 4;

  let x1 = (x0 + w).min(info.width);
  let y1 = (y0 + h).min(info.height);

  for y in y0..y1 {
    for x in x0..x1 {
      let pixel_offset = y * info.stride + x;
      let pixel_ptr = unsafe { info.base.add(pixel_offset * bytes_per_pixel) };

      unsafe {
        match info.format {
          PixelFormat::Rgb => {
            *pixel_ptr.add(0) = r;
            *pixel_ptr.add(1) = g;
            *pixel_ptr.add(2) = b;
            *pixel_ptr.add(3) = 0;
          }
          PixelFormat::Bgr => {
            *pixel_ptr.add(0) = b;
            *pixel_ptr.add(1) = g;
            *pixel_ptr.add(2) = r;
            *pixel_ptr.add(3) = 0;
          }
          PixelFormat::Unknown => {}
        }
      }
    }
  }
}