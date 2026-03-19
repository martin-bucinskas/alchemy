use lazy_static::lazy_static;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{
  InterruptDescriptorTable,
  InterruptStackFrame,
  PageFaultErrorCode,
};

use crate::gdt;
use crate::println;
use crate::threading::{self, CrashReason};

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        idt.breakpoint.set_handler_fn(breakpoint_handler);

        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }

        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.general_protection_fault
            .set_handler_fn(general_protection_fault_handler);
        idt.divide_error.set_handler_fn(divide_error_handler);
        idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);

        idt
    };
}

pub fn init() {
  IDT.load();
  println!("IDT loaded");
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
  println!();
  println!("EXCEPTION: BREAKPOINT");
  println!("{:#?}", stack_frame);

  threading::crash_current_from_exception(CrashReason::Breakpoint)
}

extern "x86-interrupt" fn divide_error_handler(stack_frame: InterruptStackFrame) {
  println!();
  println!("EXCEPTION: DIVIDE ERROR");
  println!("{:#?}", stack_frame);

  threading::crash_current_from_exception(CrashReason::DivideError)
}

extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
  println!();
  println!("EXCEPTION: INVALID OPCODE");
  println!("{:#?}", stack_frame);

  threading::crash_current_from_exception(CrashReason::InvalidOpcode)
}

extern "x86-interrupt" fn page_fault_handler(
  stack_frame: InterruptStackFrame,
  error_code: PageFaultErrorCode,
) {
  println!();
  println!("EXCEPTION: PAGE FAULT");
  println!("Accessed address: {:?}", Cr2::read());
  println!("Error code: {:?}", error_code);
  println!("{:#?}", stack_frame);

  threading::crash_current_from_exception(CrashReason::PageFault)
}

extern "x86-interrupt" fn general_protection_fault_handler(
  stack_frame: InterruptStackFrame,
  error_code: u64,
) {
  println!();
  println!("EXCEPTION: GENERAL PROTECTION FAULT");
  println!("Raw error code: {:#x}", error_code);
  println!("{:#?}", stack_frame);

  threading::crash_current_from_exception(CrashReason::GeneralProtectionFault)
}

extern "x86-interrupt" fn double_fault_handler(
  stack_frame: InterruptStackFrame,
  error_code: u64,
) -> ! {
  println!();
  println!("EXCEPTION: DOUBLE FAULT");
  println!("Error code: {}", error_code);
  println!("{:#?}", stack_frame);

  // double fault is still fatal...
  crate::cpu::hlt_loop()
}