use alchemy_bootinfo::{FramebufferInfo, PixelFormat};

#[derive(Clone, Copy)]
pub struct Framebuffer {
    pub info: FramebufferInfo,
}

unsafe impl Send for Framebuffer {}

impl Framebuffer {
    pub unsafe fn from_info(info: FramebufferInfo) -> Self {
        Self { info }
    }

    pub fn width(&self) -> usize {
        self.info.width
    }

    pub fn height(&self) -> usize {
        self.info.height
    }

    pub fn stride(&self) -> usize {
        self.info.stride
    }

    pub fn write_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }

        let pixel_offset = y * self.info.stride + x;
        let pixel_ptr = unsafe { self.info.base.add(pixel_offset * 4) };

        unsafe {
            match self.info.format {
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

    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        self.fill_rect(0, 0, self.info.width, self.info.height, r, g, b);
    }

    pub fn fill_rect(
        &mut self,
        x0: usize,
        y0: usize,
        w: usize,
        h: usize,
        r: u8,
        g: u8,
        b: u8,
    ) {
        let x1 = (x0 + w).min(self.info.width);
        let y1 = (y0 + h).min(self.info.height);

        for y in y0..y1 {
            for x in x0..x1 {
                self.write_pixel(x, y, r, g, b);
            }
        }
    }

    pub fn copy_rows_up(&mut self, src_y: usize, dst_y: usize, row_count: usize) {
        if row_count == 0 || src_y >= self.info.height || dst_y >= self.info.height {
            return;
        }

        let max_rows = self
          .info
          .height
          .saturating_sub(src_y)
          .min(self.info.height.saturating_sub(dst_y))
          .min(row_count);

        if max_rows == 0 {
            return;
        }

        let bytes_per_row = self.info.stride * 4;
        let src = unsafe { self.info.base.add(src_y * bytes_per_row) };
        let dst = unsafe { self.info.base.add(dst_y * bytes_per_row) };

        unsafe {
            core::ptr::copy(src, dst, max_rows * bytes_per_row);
        }
    }

    pub fn clear_rows(&mut self, start_y: usize, row_count: usize, r: u8, g: u8, b: u8) {
        if start_y >= self.info.height || row_count == 0 {
            return;
        }

        let end_y = (start_y + row_count).min(self.info.height);
        self.fill_rect(0, start_y, self.info.width, end_y - start_y, r, g, b);
    }
}
