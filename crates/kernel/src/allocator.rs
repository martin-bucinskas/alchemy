use core::alloc::Layout;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};

use linked_list_allocator::LockedHeap;

use crate::{println, cpu::hlt_loop};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

const HEAP_SIZE: usize = 1024 * 1024; // 1 MiB for now - probably increase at some point

#[repr(C, align(16))]
struct HeapSpace([MaybeUninit<u8>; HEAP_SIZE]);

static mut HEAP_SPACE: HeapSpace = HeapSpace([MaybeUninit::uninit(); HEAP_SIZE]);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init() {
  if INITIALIZED.swap(true, Ordering::SeqCst) {
    return;
  }

  unsafe {
    let heap_start = core::ptr::addr_of_mut!(HEAP_SPACE) as *mut u8;
    ALLOCATOR.lock().init(heap_start, HEAP_SIZE);
  }

  println!("Heap allocator initialized ({} KiB)", HEAP_SIZE / 1024);
}

#[cfg(not(test))]
#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
  println!(
    "allocation error: size={} align={}",
    layout.size(),
    layout.align()
  );
  hlt_loop()
}