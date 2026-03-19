use core::fmt::{self, Write};

pub fn e9_write_byte(byte: u8) {
  unsafe {
    core::arch::asm!(
    "out 0xE9, al",
    in("al") byte,
    options(nomem, nostack, preserves_flags)
    );
  }
}

pub fn e9_write_str(s: &str) {
  for b in s.bytes() {
    e9_write_byte(b);
  }
}

struct E9Writer;

impl Write for E9Writer {
  fn write_str(&mut self, s: &str) -> fmt::Result {
    e9_write_str(s);
    Ok(())
  }
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments<'_>) {
  let mut w = E9Writer;
  let _ = w.write_fmt(args);
}

#[macro_export]
macro_rules! ldbgprint {
    ($($arg:tt)*) => {
        $crate::debug::_print(core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! ldbgprintln {
    () => {
        $crate::debug::_print(core::format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::debug::_print(core::format_args!("{}\n", core::format_args!($($arg)*)))
    };
}