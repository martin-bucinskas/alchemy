use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use spin::Mutex;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{
  FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
  Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};

use crate::{memory, println};

const PHYS_MEM_OFFSET: AtomicU64 = AtomicU64::new(0);
const MAX_VIRT_REGIONS: usize = 128;

// Reserve a chunk of virtual address space for actor stacks.
// Stays in low-half canonical space.
const STACK_ARENA_START: u64 = 0x0000_5555_0000_0000;
const STACK_ARENA_SIZE: u64 = 256 * 1024 * 1024; // 256 MiB
const STACK_ARENA_END: u64 = STACK_ARENA_START + STACK_ARENA_SIZE;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static STACK_VIRT_ALLOCATOR: Mutex<Option<VirtualRegionAllocator>> = Mutex::new(None);

#[derive(Clone, Copy, Debug)]
struct Region {
  start: u64,
  end: u64,
}

impl Region {
  const fn empty() -> Self {
    Self { start: 0, end: 0 }
  }

  fn is_empty(&self) -> bool {
    self.end <= self.start
  }
}

struct VirtualRegionAllocator {
  regions: [Region; MAX_VIRT_REGIONS],
  region_count: usize,
}

impl VirtualRegionAllocator {
  const fn new() -> Self {
    Self {
      regions: [Region::empty(); MAX_VIRT_REGIONS],
      region_count: 0,
    }
  }

  fn add_free_region(&mut self, start: u64, end: u64) {
    let start = align_up(start, memory::PAGE_SIZE as u64);
    let end = align_down(end, memory::PAGE_SIZE as u64);

    if start >= end {
      return;
    }

    if self.region_count >= MAX_VIRT_REGIONS {
      panic!("too many virtual regions");
    }

    self.regions[self.region_count] = Region { start, end };
    self.region_count += 1;
  }

  fn compact(&mut self) {
    let mut write = 0;

    for read in 0..self.region_count {
      if !self.regions[read].is_empty() {
        self.regions[write] = self.regions[read];
        write += 1;
      }
    }

    for i in write..MAX_VIRT_REGIONS {
      self.regions[i] = Region::empty();
    }

    self.region_count = write;
  }

  fn sort_by_start(&mut self) {
    let n = self.region_count;
    for i in 1..n {
      let mut j = i;
      while j > 0 && self.regions[j - 1].start > self.regions[j].start {
        self.regions.swap(j - 1, j);
        j -= 1;
      }
    }
  }

  fn coalesce(&mut self) {
    if self.region_count <= 1 {
      return;
    }

    let mut write = 0;

    for read in 1..self.region_count {
      let current = self.regions[write];
      let next = self.regions[read];

      if current.end >= next.start {
        self.regions[write].end = current.end.max(next.end);
      } else {
        write += 1;
        self.regions[write] = next;
      }
    }

    self.region_count = write + 1;

    for i in self.region_count..MAX_VIRT_REGIONS {
      self.regions[i] = Region::empty();
    }
  }

  fn normalize(&mut self) {
    self.compact();
    self.sort_by_start();
    self.coalesce();
  }

  fn alloc_pages(&mut self, pages: usize) -> Option<u64> {
    if pages == 0 {
      return None;
    }

    let bytes = (pages as u64).checked_mul(memory::PAGE_SIZE as u64)?;

    for i in 0..self.region_count {
      let region = &mut self.regions[i];

      let alloc_start = align_up(region.start, memory::PAGE_SIZE as u64);
      let alloc_end = alloc_start.checked_add(bytes)?;

      if alloc_end <= region.end {
        region.start = alloc_end;

        if region.start >= region.end {
          self.regions[i] = Region::empty();
          self.compact();
        }

        return Some(alloc_start);
      }
    }

    None
  }

  fn free_pages(&mut self, addr: u64, pages: usize) {
    if pages == 0 {
      return;
    }

    let start = align_down(addr, memory::PAGE_SIZE as u64);
    let end = start + (pages as u64) * memory::PAGE_SIZE as u64;

    if self.region_count >= MAX_VIRT_REGIONS {
      panic!("out of virtual region slots");
    }

    self.regions[self.region_count] = Region { start, end };
    self.region_count += 1;
    self.normalize();
  }
}

pub struct GuardedStack {
  guard_base: u64,
  stack_base: u64,
  top: u64,
  pages: usize,
  phys_start: u64,
}

impl GuardedStack {
  pub fn top(&self) -> u64 {
    self.top
  }

  pub fn stack_base(&self) -> u64 {
    self.stack_base
  }

  pub fn guard_base(&self) -> u64 {
    self.guard_base
  }

  pub fn pages(&self) -> usize {
    self.pages
  }
}

impl Drop for GuardedStack {
  fn drop(&mut self) {
    unmap_range(VirtAddr::new(self.stack_base), self.pages);
    memory::free_pages(self.phys_start, self.pages);

    let total_pages = self.pages + 1; // include guard page
    let mut alloc = STACK_VIRT_ALLOCATOR.lock();
    alloc.as_mut()
      .expect("stack virtual allocator not initialized")
      .free_pages(self.guard_base, total_pages);

    println!(
      "[paging] freed guarded stack: guard=0x{:x} stack=0x{:x} pages={}",
      self.guard_base, self.stack_base, self.pages
    );
  }
}

struct KernelFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for KernelFrameAllocator {
  fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
    let phys = memory::alloc_frame()?;
    Some(PhysFrame::containing_address(PhysAddr::new(phys)))
  }
}

unsafe fn active_level_4_table(phys_offset: VirtAddr) -> &'static mut PageTable {
  let (level_4_frame, _) = Cr3::read();
  let phys = level_4_frame.start_address().as_u64();
  let virt = phys_offset + phys;
  let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
  &mut *page_table_ptr
}

unsafe fn mapper() -> OffsetPageTable<'static> {
  let phys_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(Ordering::SeqCst));
  let level_4_table = active_level_4_table(phys_offset);
  OffsetPageTable::new(level_4_table, phys_offset)
}

pub fn init(boot_info: &alchemy_bootinfo::BootInfo) {
  if INITIALIZED.swap(true, Ordering::SeqCst) {
    return;
  }

  PHYS_MEM_OFFSET.store(boot_info.physical_memory_offset, Ordering::SeqCst);

  let mut virt_alloc = VirtualRegionAllocator::new();
  virt_alloc.add_free_region(STACK_ARENA_START, STACK_ARENA_END);
  virt_alloc.normalize();

  *STACK_VIRT_ALLOCATOR.lock() = Some(virt_alloc);

  let (p4, flags) = Cr3::read();
  println!(
    "[paging] bootstrap mapper ready, CR3 P4 = 0x{:x}, flags = {:?}",
    p4.start_address().as_u64(),
    flags
  );
  println!(
    "[paging] physical memory offset = 0x{:x}",
    boot_info.physical_memory_offset
  );
  println!(
    "[paging] stack arena: 0x{:x}..0x{:x}",
    STACK_ARENA_START, STACK_ARENA_END
  );
}

pub fn alloc_stack_with_guard(stack_pages: usize) -> GuardedStack {
  assert!(stack_pages > 0, "stack_pages must be > 0");

  let total_pages = stack_pages + 1; // one unmapped guard page below the stack
  let guard_base = {
    let mut alloc = STACK_VIRT_ALLOCATOR.lock();
    alloc.as_mut()
      .expect("stack virtual allocator not initialized")
      .alloc_pages(total_pages)
      .expect("failed to allocate virtual range for guarded stack")
  };

  let stack_base = guard_base + memory::PAGE_SIZE as u64;
  let phys_start = memory::alloc_pages(stack_pages)
    .expect("failed to allocate physical pages for guarded stack");

  let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

  map_range(VirtAddr::new(stack_base), phys_start, stack_pages, flags);

  let top = stack_base + (stack_pages * memory::PAGE_SIZE) as u64;

  println!(
    "[paging] allocated guarded stack: guard=0x{:x} stack=0x{:x} top=0x{:x} pages={}",
    guard_base, stack_base, top, stack_pages
  );

  GuardedStack {
    guard_base,
    stack_base,
    top,
    pages: stack_pages,
    phys_start,
  }
}

pub fn map_range(
  virt_start: VirtAddr,
  phys_start: u64,
  pages: usize,
  flags: PageTableFlags,
) {
  assert_eq!(virt_start.as_u64() % memory::PAGE_SIZE as u64, 0);
  assert_eq!(phys_start % memory::PAGE_SIZE as u64, 0);

  let mut mapper = unsafe { mapper() };
  let mut frame_allocator = KernelFrameAllocator;

  for i in 0..pages {
    let offset = (i * memory::PAGE_SIZE) as u64;
    let page = Page::<Size4KiB>::containing_address(VirtAddr::new(
      virt_start.as_u64() + offset,
    ));
    let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(
      phys_start + offset,
    ));

    unsafe {
      mapper
        .map_to(page, frame, flags, &mut frame_allocator)
        .expect("map_to failed")
        .flush();
    }
  }
}

pub fn unmap_range(virt_start: VirtAddr, pages: usize) {
  assert_eq!(virt_start.as_u64() % memory::PAGE_SIZE as u64, 0);

  let mut mapper = unsafe { mapper() };

  for i in 0..pages {
    let offset = (i * memory::PAGE_SIZE) as u64;
    let page = Page::<Size4KiB>::containing_address(VirtAddr::new(
      virt_start.as_u64() + offset,
    ));

    let (_frame, flush) = mapper.unmap(page).expect("unmap failed");
    flush.flush();
  }
}

#[inline]
const fn align_up(value: u64, align: u64) -> u64 {
  (value + align - 1) & !(align - 1)
}

#[inline]
const fn align_down(value: u64, align: u64) -> u64 {
  value & !(align - 1)
}
