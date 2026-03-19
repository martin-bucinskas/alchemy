use core::fmt;
use core::fmt::Write;

use spin::Mutex;
use alchemy_bootinfo::FramebufferInfo;

use crate::font::{glyph, GLYPH_HEIGHT, GLYPH_WIDTH};
use crate::graphics::Framebuffer;

static WRITER: Mutex<Option<Writer>> = Mutex::new(None);

pub fn init(info: FramebufferInfo) {
  let mut writer = Writer::new(info);
  writer.clear_screen();
  *WRITER.lock() = Some(writer);
}

pub fn set_colors(fg: (u8, u8, u8), bg: (u8, u8, u8)) {
  if let Some(writer) = WRITER.lock().as_mut() {
    writer.fg = fg;
    writer.bg = bg;
  }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments<'_>) {
  let mut guard = WRITER.lock();
  if let Some(writer) = guard.as_mut() {
    let _ = writer.write_fmt(args);
  }
}

pub struct Writer {
  fb: Framebuffer,
  col: usize,
  row: usize,
  cols: usize,
  rows: usize,
  fg: (u8, u8, u8),
  bg: (u8, u8, u8),
}

impl Writer {
  pub fn new(info: FramebufferInfo) -> Self {
    let cols = info.width / GLYPH_WIDTH;
    let rows = info.height / GLYPH_HEIGHT;

    Self {
      fb: unsafe { Framebuffer::from_info(info) },
      col: 0,
      row: 0,
      cols,
      rows,
      fg: (255, 255, 255),
      bg: (0, 0, 0),
    }
  }

  pub fn clear_screen(&mut self) {
    let (r, g, b) = self.bg;
    self.fb.clear(r, g, b);
    self.col = 0;
    self.row = 0;
  }

  fn newline(&mut self) {
    self.col = 0;
    self.row += 1;

    if self.row >= self.rows {
      self.scroll_up(1);
      self.row = self.rows - 1;
    }
  }

  fn scroll_up(&mut self, text_rows: usize) {
    let pixel_rows = text_rows * GLYPH_HEIGHT;
    if pixel_rows == 0 {
      return;
    }

    if pixel_rows >= self.fb.height() {
      self.clear_screen();
      return;
    }

    let height_to_keep = self.fb.height() - pixel_rows;
    self.fb.copy_rows_up(pixel_rows, 0, height_to_keep);

    let (br, bg, bb) = self.bg;
    self.fb.clear_rows(height_to_keep, pixel_rows, br, bg, bb);
  }

  fn draw_byte(&mut self, byte: u8) {
    if self.col >= self.cols {
      self.newline();
    }

    let glyph_rows = glyph(byte);
    let px = self.col * GLYPH_WIDTH;
    let py = self.row * GLYPH_HEIGHT;

    let (fr, fg, fb) = self.fg;
    let (br, bg, bb) = self.bg;

    for (gy, row_bits) in glyph_rows.iter().copied().enumerate() {
      for gx in 0..8 {
        let bit = (row_bits & (0x80 >> gx)) != 0;
        if bit {
          self.fb.write_pixel(px + gx, py + gy, fr, fg, fb);
        } else {
          self.fb.write_pixel(px + gx, py + gy, br, bg, bb);
        }
      }
    }

    self.col += 1;
  }

  fn write_byte(&mut self, byte: u8) {
    match byte {
      b'\n' => self.newline(),
      b'\r' => self.col = 0,
      b'\t' => {
        for _ in 0..4 {
          self.write_byte(b' ');
        }
      }
      0x20..=0x7e => self.draw_byte(byte),
      _ => self.draw_byte(b'?'),
    }
  }
}

impl Write for Writer {
  fn write_str(&mut self, s: &str) -> fmt::Result {
    for byte in s.bytes() {
      self.write_byte(byte);
    }
    Ok(())
  }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::console::_print(core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::console::_print(core::format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::console::_print(core::format_args!("{}\n", core::format_args!($($arg)*)))
    };
}
