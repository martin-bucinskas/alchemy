use uefi::boot::{self, AllocateType, MemoryType};
use uefi::mem::memory_map::MemoryMap;
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame};
use x86_64::PhysAddr;

pub const PHYSICAL_MEMORY_OFFSET: u64 = 0xffff_8000_0000_0000;

const SIZE_2M: u64 = 2 * 1024 * 1024;
const SIZE_1G: u64 = 1024 * 1024 * 1024;

fn p4_index(addr: u64) -> usize {
  ((addr >> 39) & 0x1ff) as usize
}

fn p3_index(addr: u64) -> usize {
  ((addr >> 30) & 0x1ff) as usize
}

const fn align_up(value: u64, align: u64) -> u64 {
  (value + align - 1) & !(align - 1)
}

unsafe fn alloc_zeroed_page_table() -> (u64, &'static mut PageTable) {
  let ptr = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1)
    .expect("failed to allocate page table")
    .cast::<PageTable>();

  core::ptr::write_bytes(ptr.as_ptr().cast::<u8>(), 0, 4096);

  (ptr.as_ptr() as u64, &mut *ptr.as_ptr())
}

pub fn highest_phys_end() -> u64 {
  let mmap = boot::memory_map(MemoryType::LOADER_DATA)
    .expect("failed to fetch memory map");

  mmap.entries()
    .map(|d| d.phys_start + d.page_count * 4096)
    .max()
    .unwrap_or(0)
}

pub fn build_kernel_page_tables(max_phys_end: u64) -> u64 {
  let map_end = align_up(max_phys_end, SIZE_2M);
  let gig_chunks = align_up(map_end, SIZE_1G) / SIZE_1G;

  let (p4_phys, p4) = unsafe { alloc_zeroed_page_table() };
  let (id_p3_phys, id_p3) = unsafe { alloc_zeroed_page_table() };
  let (off_p3_phys, off_p3) = unsafe { alloc_zeroed_page_table() };

  let table_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
  let huge_flags = table_flags | PageTableFlags::HUGE_PAGE;

  // Identity map low physical memory
  p4[p4_index(0)].set_addr(PhysAddr::new(id_p3_phys), table_flags);

  // Map all physical memory at PHYSICAL_MEMORY_OFFSET
  p4[p4_index(PHYSICAL_MEMORY_OFFSET)].set_addr(PhysAddr::new(off_p3_phys), table_flags);

  for gig in 0..gig_chunks {
    let base = gig * SIZE_1G;

    let (id_p2_phys, id_p2) = unsafe { alloc_zeroed_page_table() };
    let (off_p2_phys, off_p2) = unsafe { alloc_zeroed_page_table() };

    id_p3[p3_index(base)].set_addr(PhysAddr::new(id_p2_phys), table_flags);
    off_p3[p3_index(PHYSICAL_MEMORY_OFFSET + base)]
      .set_addr(PhysAddr::new(off_p2_phys), table_flags);

    for i in 0..512u64 {
      let phys = base + i * SIZE_2M;
      if phys >= map_end {
        break;
      }

      id_p2[i as usize].set_addr(PhysAddr::new(phys), huge_flags);
      off_p2[i as usize].set_addr(PhysAddr::new(phys), huge_flags);
    }
  }

  p4_phys
}

pub unsafe fn switch_to_page_tables(p4_phys: u64) {
  let frame = PhysFrame::containing_address(PhysAddr::new(p4_phys));
  Cr3::write(frame, Cr3Flags::empty());
}