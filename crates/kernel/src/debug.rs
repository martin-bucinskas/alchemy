use core::fmt::{self, Write};
use x86_64::instructions::port::Port;

pub fn e9_write_byte(byte: u8) {
  unsafe {
    let mut port = Port::<u8>::new(0xE9);
    port.write(byte);
  }
}

pub fn e9_write_str(s: &str) {
  for byte in s.bytes() {
    e9_write_byte(byte);
  }
}

struct E9Writer;

impl Write for E9Writer {
  fn write_str(&mut self, s: &str) -> fmt::Result {
    e9_write_str(s);
    Ok(())
  }
}

pub fn _print(args: fmt::Arguments<'_>) {
  let mut writer = E9Writer;
  let _ = writer.write_fmt(args);
}

#[macro_export]
macro_rules! dbgprint {
    ($($arg:tt)*) => {
        $crate::debug::_print(core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! dbgprintln {
    () => {
        $crate::debug::_print(core::format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::debug::_print(core::format_args!("{}\n", core::format_args!($($arg)*)))
    };
}