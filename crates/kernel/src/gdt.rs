use lazy_static::lazy_static;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(STACK));
            let stack_end = stack_start + STACK_SIZE as u64;
            stack_end
        };
        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        let data_selector = gdt.append(Descriptor::kernel_data_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));

        (
            gdt,
            Selectors {
                code_selector,
                data_selector,
                tss_selector,
            },
        )
    };
}

struct Selectors {
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

pub fn init() {
    unsafe {
        x86_64::instructions::interrupts::disable();

        GDT.0.load();

        core::arch::asm!(
        "push {0:r}",
        "lea {1:r}, [2f + rip]",
        "push {1:r}",
        "retfq",
        "2:",
        "mov ss, {2:e}",
        "mov ds, {2:e}",
        "mov es, {2:e}",
        "xor {1:r}, {1:r}",
        "mov fs, {1:e}",
        "mov gs, {1:e}",
        in(reg) GDT.1.code_selector.0 as u64,
        lateout(reg) _,
        in(reg) GDT.1.data_selector.0 as u32,
        );
    }
}

pub fn init_tss() {
    use x86_64::instructions::tables::load_tss;

    unsafe {
        load_tss(GDT.1.tss_selector);
    }
}