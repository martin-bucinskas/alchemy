#![no_std]
#![no_main]

mod graphics;
mod debug;

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use core::mem::ManuallyDrop;
use core::panic::PanicInfo;
use core::ptr::{copy_nonoverlapping, write_bytes};
use uefi::{cstr16};
use uefi::boot::{AllocateType, MemoryType};
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode};
use xmas_elf::ElfFile;
use xmas_elf::program::{ProgramHeader, Type};
use alchemy_bootinfo::{BootInfo, FramebufferInfo, PixelFormat};
use crate::graphics::{clear_framebuffer, draw_loader_test_pattern, fill_rect, probe_framebuffer};

const PAGE_SIZE: usize = 4096;
const KERNEL_PATH: &uefi::CStr16 = cstr16!(r"\kernel.elf");

#[entry]
fn main() -> Status {
    uefi::helpers::init()
      .expect("failed to initialize UEFI helpers");
    ldbgprintln!("L0: loader entered");

    let framebuffer = probe_framebuffer();
    ldbgprintln!(
        "L1: framebuffer {}x{} stride={}",
        framebuffer.width,
        framebuffer.height,
        framebuffer.stride
    );
    draw_loader_test_pattern(framebuffer);
    clear_framebuffer(framebuffer, 0, 0, 40);
    fill_rect(framebuffer, 20, 20, 30, 30, 255, 0, 0); // stage 1

    let boot_info_ptr = allocate_boot_info_slot();
    ldbgprintln!("L2: boot info slot allocated at {:p}", boot_info_ptr);
    fill_rect(framebuffer, 60, 20, 30, 30, 255, 255, 0); // stage 2

    let kernel_entry = {
        let kernel_bytes = read_kernel_file(KERNEL_PATH);
        ldbgprintln!("L3: read kernel.elf ({} bytes)", kernel_bytes.len());
        fill_rect(framebuffer, 100, 20, 30, 30, 0, 255, 255); // stage 3

        let entry = load_elf_kernel(&kernel_bytes);
        ldbgprintln!("L4: ELF loaded, entry = 0x{:x}", entry);
        fill_rect(framebuffer, 140, 20, 30, 30, 255, 0, 255); // stage 4

        entry
    };

    ldbgprintln!("L5: exiting boot services");
    fill_rect(framebuffer, 180, 20, 30, 30, 0, 255, 0); // stage 5 before EBS

    let memory_map = ManuallyDrop::new(unsafe { boot::exit_boot_services(None) });

    unsafe {
        boot_info_ptr.write(BootInfo {
            framebuffer,
            memory_map_ptr: memory_map.buffer().as_ptr(),
            memory_map_len: memory_map.buffer().len(),
            memory_map_desc_size: memory_map.meta().desc_size,
            memory_map_desc_version: memory_map.meta().desc_version,
        });
    }

    ldbgprintln!("L6: jumping to kernel entry 0x{:x}", kernel_entry);

    unsafe {
        jump_to_kernel(kernel_entry, boot_info_ptr.cast_const());
    }
}

fn read_kernel_file(path: &uefi::CStr16) -> Vec<u8> {
    let mut fs = boot::get_image_file_system(boot::image_handle())
      .expect("failed to get boot filesystem");
    let mut root = fs.open_volume()
      .expect("failed to open boot volume");

    let handle = root
      .open(path, FileMode::Read, FileAttribute::empty())
      .expect("failed to open kernel.elf");

    let mut file = handle.into_regular_file()
      .expect("kernel.elf is not a regular file");

    let file_info = file
      .get_boxed_info::<FileInfo>()
      .expect("failed to read file info");

    let file_size = file_info.file_size() as usize;

    let mut bytes = vec![0u8; file_size];
    let mut read_total = 0;

    while read_total < bytes.len() {
        let n = file.read(&mut bytes[read_total..])
          .expect("failed while reading kernel.elf");

        if n == 0 {
            break;
        }

        read_total += n;
    }

    bytes.truncate(read_total);
    bytes
}

fn load_elf_kernel(bytes: &[u8]) -> u64 {
    dump_memory_map();
    let elf = ElfFile::new(bytes)
      .expect("kernel.elf is not valid ELF");

    let entry = elf.header.pt2.entry_point();
    ldbgprintln!("ELF entry = 0x{:x}", entry);

    let mut image_start = usize::MAX;
    let mut image_end = 0usize;

    for (i, ph) in elf.program_iter().enumerate() {
        ldbgprintln!(
            "PH{}: type={:?} off=0x{:x} vaddr=0x{:x} filesz=0x{:x} memsz=0x{:x} align=0x{:x}",
            i,
            ph.get_type(),
            ph.offset(),
            ph.virtual_addr(),
            ph.file_size(),
            ph.mem_size(),
            ph.align(),
        );

        if ph.get_type() == Ok(Type::Load) {
            let seg_start = align_down(ph.virtual_addr() as usize, PAGE_SIZE);
            let seg_end = align_up((ph.virtual_addr() as usize) + (ph.mem_size() as usize), PAGE_SIZE);

            image_start = image_start.min(seg_start);
            image_end = image_end.max(seg_end);
        }
    }

    assert!(image_start < image_end, "no PT_LOAD segments found");

    let page_count = (image_end - image_start) / PAGE_SIZE;

    ldbgprintln!(
        "IMAGE: start=0x{:x} end=0x{:x} pages={}",
        image_start,
        image_end,
        page_count
    );

    boot::allocate_pages(
        AllocateType::Address(image_start as u64),
        MemoryType::LOADER_DATA,
        page_count,
    )
      .expect("failed to allocate kernel image span");

    unsafe {
        write_bytes(image_start as *mut u8, 0, image_end - image_start);
    }

    ldbgprintln!("IMAGE: allocation ok, zeroed");

    for (i, ph) in elf.program_iter().enumerate() {
        if ph.get_type() == Ok(Type::Load) {
            copy_segment(i, &ph, bytes);
        }
    }

    entry
}

fn copy_segment(index: usize, ph: &ProgramHeader<'_>, bytes: &[u8]) {
    let virt_addr = ph.virtual_addr() as usize;
    let mem_size = ph.mem_size() as usize;
    let file_size = ph.file_size() as usize;
    let offset = ph.offset() as usize;

    ldbgprintln!(
        "COPY{}: vaddr=0x{:x} mem=0x{:x} file=0x{:x} off=0x{:x}",
        index, virt_addr, mem_size, file_size, offset
    );

    if mem_size == 0 {
        ldbgprintln!("COPY{}: skipped (mem_size=0)", index);
        return;
    }

    assert!(file_size <= mem_size, "ELF segment file size > mem size");

    unsafe {
        copy_nonoverlapping(
            bytes.as_ptr().add(offset),
            virt_addr as *mut u8,
            file_size,
        );
    }

    ldbgprintln!("COPY{}: copied ok", index);
}

fn dump_memory_map() {
    let mmap = boot::memory_map(MemoryType::LOADER_DATA)
      .expect("failed to fetch memory map");

    ldbgprintln!("---- MEMORY MAP ----");
    for (i, desc) in mmap.entries().enumerate() {
        let start = desc.phys_start;
        let size = desc.page_count * 4096;
        let end = start + size;
        ldbgprintln!(
            "#{:02} type={:?} start=0x{:x} end=0x{:x} pages={}",
            i,
            desc.ty,
            start,
            end,
            desc.page_count
        );
    }
    ldbgprintln!("--------------------");
}

fn allocate_boot_info_slot() -> *mut BootInfo {
    let ptr = boot::allocate_pool(MemoryType::LOADER_DATA, size_of::<BootInfo>())
      .expect("failed to allocate boot info slot")
      .as_ptr();

    ptr.cast::<BootInfo>()
}

#[inline]
const fn align_down(value: usize, align: usize) -> usize {
    value & !(align - 1)
}

#[inline]
const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

type KernelEntry = unsafe extern "sysv64" fn(*const BootInfo) -> !;

unsafe fn jump_to_kernel(entry_addr: u64, boot_info: *const BootInfo) -> ! {
    let entry: KernelEntry = unsafe { core::mem::transmute(entry_addr as usize) };
    unsafe { entry(boot_info) }
}
