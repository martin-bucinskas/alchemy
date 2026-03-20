use core::alloc::Layout;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};

use linked_list_allocator::LockedHeap;
use x86_64::structures::paging::PageTableFlags;
use x86_64::VirtAddr;
use crate::{println, cpu::hlt_loop, paging};
use crate::memory::{alloc_pages, PAGE_SIZE};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

static INITIALIZED: AtomicBool = AtomicBool::new(false);

const HEAP_PAGES: usize = 256; // 256 = 1 MiB
const HEAP_START: u64 = 0x_4444_4444_0000;

pub fn init() {
  if INITIALIZED.swap(true, Ordering::SeqCst) {
    return;
  }
  
  let heap_phys_start = alloc_pages(HEAP_PAGES)
    .expect("failed to allocate physical pages for kernel heap");
  let heap_size = HEAP_PAGES * PAGE_SIZE;

  let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
  
  paging::map_range(
    VirtAddr::new(HEAP_START),
    heap_phys_start,
    HEAP_PAGES,
    flags,
  );

  unsafe {
    core::ptr::write_bytes(HEAP_START as *mut u8, 0, heap_size);
    ALLOCATOR.lock().init(HEAP_START as *mut u8, heap_size);
  }

  println!(
    "[allocator] heap initialized: virt=0x{:x} phys=0x{:x} ({} KiB)",
    HEAP_START,
    heap_phys_start,
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
