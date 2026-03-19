use core::marker::PhantomData;
use core::mem::size_of;
use core::ptr::read_unaligned;

use alchemy_bootinfo::BootInfo;
use spin::Mutex;

use crate::println;

pub const PAGE_SIZE: usize = 4096;
const PAGE_SIZE_U64: u64 = PAGE_SIZE as u64;
const MAX_REGIONS: usize = 128;
const EFI_CONVENTIONAL_MEMORY: u32 = 7;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UefiMemoryDescriptor {
  pub ty: u32,
  _pad: u32,
  pub phys_start: u64,
  pub virt_start: u64,
  pub page_count: u64,
  pub att: u64,
}

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

  fn size_bytes(&self) -> u64 {
    self.end.saturating_sub(self.start)
  }

  fn size_frames(&self) -> u64 {
    self.size_bytes() / PAGE_SIZE_U64
  }
}

pub struct MemoryMapIter<'a> {
  base: *const u8,
  len: usize,
  desc_size: usize,
  offset: usize,
  _marker: PhantomData<&'a u8>,
}

impl<'a> MemoryMapIter<'a> {
  pub fn new(boot_info: &'a BootInfo) -> Self {
    Self {
      base: boot_info.memory_map_ptr,
      len: boot_info.memory_map_len,
      desc_size: boot_info.memory_map_desc_size,
      offset: 0,
      _marker: PhantomData,
    }
  }
}

impl Iterator for MemoryMapIter<'_> {
  type Item = UefiMemoryDescriptor;

  fn next(&mut self) -> Option<Self::Item> {
    if self.offset + self.desc_size > self.len {
      return None;
    }

    let ptr = unsafe { self.base.add(self.offset) as *const UefiMemoryDescriptor };
    self.offset += self.desc_size;

    Some(unsafe { read_unaligned(ptr) })
  }
}

pub struct PhysicalFrameAllocator {
  regions: [Region; MAX_REGIONS],
  region_count: usize,
}

impl PhysicalFrameAllocator {
  const fn new() -> Self {
    Self {
      regions: [Region::empty(); MAX_REGIONS],
      region_count: 0,
    }
  }

  fn add_free_region(&mut self, start: u64, end: u64) {
    let start = align_up_u64(start.max(PAGE_SIZE_U64), PAGE_SIZE_U64);
    let end = align_down_u64(end, PAGE_SIZE_U64);

    if start >= end {
      return;
    }

    if self.region_count >= MAX_REGIONS {
      panic!("too many free memory regions for early allocator");
    }

    self.regions[self.region_count] = Region { start, end };
    self.region_count += 1;
  }

  fn normalize(&mut self) {
    self.compact();
    self.sort_by_start();
    self.coalesce();
  }

  fn compact(&mut self) {
    let mut write = 0;

    for read in 0..self.region_count {
      if !self.regions[read].is_empty() {
        self.regions[write] = self.regions[read];
        write += 1;
      }
    }

    for i in write..MAX_REGIONS {
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

    for i in self.region_count..MAX_REGIONS {
      self.regions[i] = Region::empty();
    }
  }

  fn alloc_pages(&mut self, pages: usize) -> Option<u64> {
    if pages == 0 {
      return None;
    }

    let bytes = (pages as u64).checked_mul(PAGE_SIZE_U64)?;

    for i in 0..self.region_count {
      let region = &mut self.regions[i];

      let alloc_start = align_up_u64(region.start, PAGE_SIZE_U64);
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

    let start = align_down_u64(addr, PAGE_SIZE_U64);
    let end = start + (pages as u64) * PAGE_SIZE_U64;

    if self.region_count >= MAX_REGIONS {
      panic!("out of region slots while freeing pages");
    }

    self.regions[self.region_count] = Region { start, end };
    self.region_count += 1;
    self.normalize();
  }

  fn free_frames_remaining(&self) -> u64 {
    self.regions[..self.region_count]
      .iter()
      .map(Region::size_frames)
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
      allocator.add_free_region(start, end);
    }
  }

  allocator.normalize();

  let region_count = allocator.region_count();
  let free_frames = allocator.free_frames_remaining();
  let free_mib = (free_frames * 4) / 1024;

  *FRAME_ALLOCATOR.lock() = Some(allocator);

  println!(
    "[memory] parsed {} conventional regions, {} frames ({} MiB usable)",
    region_count, free_frames, free_mib
  );
}

pub fn alloc_frame() -> Option<u64> {
  alloc_pages(1)
}

pub fn alloc_pages(pages: usize) -> Option<u64> {
  FRAME_ALLOCATOR.lock().as_mut()?.alloc_pages(pages)
}

pub fn free_pages(addr: u64, pages: usize) {
  FRAME_ALLOCATOR
    .lock()
    .as_mut()
    .expect("physical frame allocator not initialized")
    .free_pages(addr, pages);
}

#[inline]
const fn align_up_u64(value: u64, align: u64) -> u64 {
  (value + align - 1) & !(align - 1)
}

#[inline]
const fn align_down_u64(value: u64, align: u64) -> u64 {
  value & !(align - 1)
}