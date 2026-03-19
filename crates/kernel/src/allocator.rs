use core::alloc::Layout;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};

use linked_list_allocator::LockedHeap;

use crate::{println, cpu::hlt_loop};
use crate::memory::{alloc_pages, PAGE_SIZE};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

static INITIALIZED: AtomicBool = AtomicBool::new(false);

const HEAP_PAGES: usize = 256; // 256 = 1 MiB

pub fn init() {
  if INITIALIZED.swap(true, Ordering::SeqCst) {
    return;
  }

  let heap_start = alloc_pages(HEAP_PAGES)
    .expect("failed to allocate physical pages for kernel heap");
  let heap_size = HEAP_PAGES * PAGE_SIZE;

  unsafe {
    core::ptr::write_bytes(heap_start as *mut u8, 0, heap_size);
    ALLOCATOR.lock().init(heap_start as *mut u8, heap_size);
  }

  println!(
    "[allocator] heap initialized at 0x{:x} ({} KiB)",
    heap_start,
    heap_size / 1024
  );
}

#[cfg(not(test))]
#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
  println!(
    "[allocator] allocation error: size={} align={}",
    layout.size(),
    layout.align()
  );
  hlt_loop()
}
