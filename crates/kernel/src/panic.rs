use core::panic::PanicInfo;

use crate::{dbgprintln, println};

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
  dbgprintln!();
  dbgprintln!("================ PANIC ================");
  dbgprintln!("{}", info);
  dbgprintln!("=======================================");

  // this aint safe for panic paths since print has locks - easy to get into deadlocks
  // println!();
  // println!("================ PANIC ================");
  // println!("{}", info);
  // println!("=======================================");

  // if actor, we treat panic as an actor crash
  if crate::threading::panicking_actor_id().is_some() {
    crate::threading::panic_current();
  }

  // else its more like kernel-level panic and we should panic as things are on fire
  crate::cpu::hlt_loop()
}