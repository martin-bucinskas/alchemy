#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use alchemy_bootinfo::BootInfo;
use crate::threading::Message;

mod allocator;
mod console;
mod cpu;
mod font;
mod gdt;
mod graphics;
mod idt;
mod panic;
mod threading;
mod debug;

static WORKER_ID: AtomicUsize = AtomicUsize::new(0);
static SUPERVISOR_ID: AtomicUsize = AtomicUsize::new(0);

fn supervisor_actor() {
    println!("[supervisor] started");

    loop {
        match threading::receive() {
            Message::ChildCrashed { child, reason } => {
                println!("[supervisor] child {} crashed: {:?}", child, reason);

                let new_child = threading::spawn(worker_actor, threading::current_actor_id());
                WORKER_ID.store(new_child, Ordering::SeqCst);
                println!("[supervisor] restarted worker as {}", new_child);
            }
            Message::Bytes(bytes) => {
                println!("[supervisor] got {} bytes", bytes.len());
            }
        }

        threading::yield_now();
    }
}

#[allow(deref_nullptr)]
fn worker_actor() {
    println!("[worker {}] started", threading::current_actor_id().unwrap());

    let payload: Box<[u8]> = vec![1, 2, 3, 4, 5].into_boxed_slice();
    let supervisor = SUPERVISOR_ID.load(Ordering::SeqCst);
    let _ = threading::send(supervisor, Message::Bytes(payload));

    println!("[worker] forcing a crash...");
    unsafe {
        *(0 as *mut u64) = 0xdead_beef;
    }

    loop {
        threading::yield_now();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start(boot_info: *const BootInfo) -> ! {
    dbgprintln!("kernel: entered _start");
    let boot_info = unsafe { &*boot_info };

    let mut fb = unsafe { crate::graphics::Framebuffer::from_info(boot_info.framebuffer) };
    fb.clear(0, 80, 0);

    console::init(boot_info.framebuffer);
    println!("[kernel] console initalized");

    allocator::init();
    println!("[kernel] allocator initalized");

    gdt::init();
    println!("[kernel] gdt initalized");

    idt::init();
    println!("[kernel] idt initalized");

    println!("[kernel] alchemy kernel started");
    println!(
        "Framebuffer: {}x{} stride={}",
        boot_info.framebuffer.width,
        boot_info.framebuffer.height,
        boot_info.framebuffer.stride
    );

    let supervisor = threading::spawn(supervisor_actor, None);
    SUPERVISOR_ID.store(supervisor, Ordering::SeqCst);

    let worker = threading::spawn(worker_actor, Some(supervisor));
    WORKER_ID.store(worker, Ordering::SeqCst);

    println!("spawned supervisor={} worker={}", supervisor, worker);

    threading::run()

    // loop {
    //     x86_64::instructions::hlt();
    // }
}