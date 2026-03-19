use core::panic::PanicInfo;

use crate::println;

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
  println!();
  println!("================ PANIC ================");
  println!("{}", info);
  println!("=======================================");

  loop {
    x86_64::instructions::hlt();
  }
}