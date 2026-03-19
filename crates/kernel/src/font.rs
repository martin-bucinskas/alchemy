pub const GLYPH_WIDTH: usize = 8;
pub const GLYPH_HEIGHT: usize = 16;
pub const GLYPH_COUNT: usize = 256;
pub const FONT_SIZE: usize = GLYPH_COUNT * GLYPH_HEIGHT;

// static FONT: &[u8; FONT_SIZE] = include_bytes!("../assets/XGA_8x16.bin");
// static FONT: &[u8; FONT_SIZE] = include_bytes!("../assets/vgaedge16__8x16.bin");
static FONT: &[u8; FONT_SIZE] = include_bytes!("../assets/IBM_VGA_8x16.bin");

pub fn glyph(byte: u8) -> &'static [u8] {
  let start = byte as usize * GLYPH_HEIGHT;
  let end = start + GLYPH_HEIGHT;
  &FONT[start..end]
}