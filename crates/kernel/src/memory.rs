use core::marker::PhantomData;
use core::ptr::read_unaligned;
use spin::Mutex;
use alchemy_bootinfo::BootInfo;
use crate::println;

pub const PAGE_SIZE: usize = 4096;
const PAGE_SIZE_U64: u64 = PAGE_SIZE as u64;
const MAX_REGIONS: usize = 128;
const EFI_CONVENTIONAL_MEMORY: u32 = 7;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct UefiMemoryDescriptor {
  pub ty: u32,
  _pad: u32,
  pub phys_start: u64,
  pub virt_start: u64,
  pub page_count: u64,
  pub att: u64,
}

#[derive(Copy, Clone, Debug)]
struct Region {
  start: u64,
  end: u64,
  next: u64,
}

impl Region {
  const fn empty() -> Self {
    Self {
      start: 0,
      end: 0,
      next: 0,
    }
  }

  fn is_empty(&self) -> bool {
    self.end <= self.start
  }

  fn bytes_remaining(&self) -> u64 {
    self.end.saturating_sub(self.next)
  }

  fn frames_remaining(&self) -> u64 {
    self.bytes_remaining() / PAGE_SIZE_U64
  }
}

pub struct MemoryMapIter<'a> {
  base: *const u8,
  len: usize,
  desc_size: usize,
  offset: usize,
  _market: PhantomData<&'a u8>,
}

impl<'a> MemoryMapIter<'a> {
  pub fn new(boot_info: &'a BootInfo) -> Self {
    Self {
      base: boot_info.memory_map_ptr,
      len: boot_info.memory_map_len,
      desc_size: boot_info.memory_map_desc_size,
      offset: 0,
      _market: PhantomData,
    }
  }
}

impl Iterator for MemoryMapIter<'_> {
  type Item = UefiMemoryDescriptor;

  fn next(&mut self) -> Option<Self::Item> {
    if self.offset + self.desc_size > self.len {
      return None;
    }

    let ptr = unsafe {
      self.base.add(self.offset) as *const UefiMemoryDescriptor
    };
    self.offset += self.desc_size;

    Some(unsafe { read_unaligned(ptr) })
  }
}

pub struct PhysicalFrameAllocator {
  regions: [Region; MAX_REGIONS],
  region_count: usize,
  total_frames: u64,
}

impl PhysicalFrameAllocator {
  const fn new() -> Self {
    Self {
      regions: [Region::empty(); MAX_REGIONS],
      region_count: 0,
      total_frames: 0,
    }
  }

  fn add_region(&mut self, start: u64, end: u64) {
    if start >= end {
      return;
    }

    if self.region_count >= MAX_REGIONS {
      panic!("too many memory regions for early allocator");
    }

    let start = align_up_u64(start.max(PAGE_SIZE_U64), PAGE_SIZE_U64);
    let end = align_down_u64(end, PAGE_SIZE_U64);

    if start >= end {
      return;
    }

    let region = Region {
      start,
      end,
      next: start,
    };

    self.total_frames += (end - start) / PAGE_SIZE_U64;
    self.regions[self.region_count] = region;
    self.region_count += 1;
  }

  fn alloc_pages(&mut self, pages: usize) -> Option<u64> {
    let bytes = (pages as u64) * PAGE_SIZE_U64;

    for region in &mut self.regions[..self.region_count] {
      let next = align_up_u64(region.next, PAGE_SIZE_U64);
      let end = next.checked_add(bytes)?;

      if end <= region.end {
        region.next = end;
        return Some(next);
      }
    }

    None
  }

  fn free_frames_remaining(&self) -> u64 {
    self.regions[..self.region_count]
      .iter()
      .map(Region::frames_remaining)
      .sum()
  }

  fn region_count(&self) -> usize {
    self.region_count
  }
}

static FRAME_ALLOCATOR: Mutex<Option<PhysicalFrameAllocator>> = Mutex::new(None);

pub fn init(boot_info: &BootInfo) {
  if boot_info.memory_map_desc_size < size_of::<UefiMemoryDescriptor>() {
    panic!("memory map descriptor size too small");
  }

  let mut allocator = PhysicalFrameAllocator::new();

  for desc in MemoryMapIter::new(boot_info) {
    if desc.ty == EFI_CONVENTIONAL_MEMORY && desc.page_count > 0 {
      let start = desc.phys_start;
      let end = desc.phys_start + desc.page_count * PAGE_SIZE_U64;
      allocator.add_region(start, end);
    }
  }

  let region_count = allocator.region_count();
  let free_frames = allocator.free_frames_remaining();
  let free_mib = (free_frames * 4) / 1024;

  *FRAME_ALLOCATOR.lock() = Some(allocator);

  println!(
    "[memory] parsed {} conventional regions, {} frames ({} MiB usable)",
    region_count,
    free_frames,
    free_mib
  );
}

pub fn alloc_frame() -> Option<u64> {
  alloc_pages(1)
}

pub fn alloc_pages(pages: usize) -> Option<u64> {
  FRAME_ALLOCATOR
    .lock()
    .as_mut()
    .and_then(|alloc| alloc.alloc_pages(pages))
}

#[inline]
const fn align_up_u64(value: u64, align: u64) -> u64 {
  (value + align - 1) & !(align - 1)
}

#[inline]
const fn align_down_u64(value: u64, align: u64) -> u64 {
  value & !(align - 1)
}
