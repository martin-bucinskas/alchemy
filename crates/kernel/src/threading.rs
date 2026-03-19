use alloc::{
    boxed::Box,
    collections::VecDeque,
    vec::Vec,
};
use core::arch::global_asm;
use core::sync::atomic::{AtomicUsize, Ordering};

use spin::Mutex;

use crate::cpu::hlt_loop;
use crate::memory;
use crate::println;

pub type ActorId = usize;
pub type ActorEntry = fn();

const DEFAULT_STACK_PAGES: usize = 4; // 16 KiB

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorState {
    Ready,
    Running,
    Waiting,
    Dead,
    Crashed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashReason {
    DivideError,
    GeneralProtectionFault,
    PageFault,
    InvalidOpcode,
    Breakpoint,
    Returned,
    Panic,
    Unknown,
}

#[derive(Debug)]
pub enum Message {
    Bytes(Box<[u8]>),
    ChildCrashed {
        child: ActorId,
        reason: CrashReason,
    },
}

#[derive(Default, Debug)]
#[repr(C)]
struct ActorContext {
    rsp: u64,
}

struct ActorStack {
    base: *mut u8,
    pages: usize,
}

unsafe impl Send for ActorStack {}

impl ActorStack {
    fn new(pages: usize) -> Self {
        let addr = memory::alloc_pages(pages)
          .expect("failed to allocate actor stack pages");
        let size = pages * memory::PAGE_SIZE;

        unsafe {
            core::ptr::write_bytes(addr as *mut u8, 0, size);
        }

        Self {
            base: addr as *mut u8,
            pages,
        }
    }

    fn top(&self) -> u64 {
        unsafe { self.base.add(self.pages * memory::PAGE_SIZE) as u64 }
    }
}

impl Drop for ActorStack {
    fn drop(&mut self) {
        memory::free_pages(self.base as u64, self.pages);
    }
}

pub struct Actor {
    id: ActorId,
    supervisor: Option<ActorId>,
    state: ActorState,
    context: ActorContext,
    stack: Option<ActorStack>,
    mailbox: VecDeque<Message>,
    entry: ActorEntry,
    cleaned: bool,
}

impl Actor {
    fn cleanup(&mut self) {
        if self.cleaned {
            return;
        }
        self.mailbox.clear();
        self.stack = None;
        self.cleaned = true;
    }
}

struct Scheduler {
    actors: Vec<Actor>,
    current: Option<usize>,
    scheduler_rsp: u64,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            actors: Vec::new(),
            current: None,
            scheduler_rsp: 0,
        }
    }

    fn actor_index_by_id(&self, id: ActorId) -> Option<usize> {
        self.actors.iter().position(|a| a.id == id)
    }

    fn pick_next_ready(&self, after: Option<usize>) -> Option<usize> {
        if self.actors.is_empty() {
            return None;
        }

        let start = after.map(|i| i + 1).unwrap_or(0);
        let len = self.actors.len();

        for offset in 0..len {
            let idx = (start + offset) % len;
            if self.actors[idx].state == ActorState::Ready {
                return Some(idx);
            }
        }

        None
    }

    fn reap_terminated(&mut self) {
        let current = self.current;
        for (idx, actor) in self.actors.iter_mut().enumerate() {
            if Some(idx) == current {
                continue;
            }

            match actor.state {
                ActorState::Dead | ActorState::Crashed => actor.cleanup(),
                _ => {}
            }
        }
    }
}

static NEXT_ACTOR_ID: AtomicUsize = AtomicUsize::new(1);
static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

global_asm!(
    r#"
    .global alchemy_context_switch
alchemy_context_switch:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov rsp, rsi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret

    .global alchemy_jump_to
alchemy_jump_to:
    mov rsp, rdi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
"#
);

unsafe extern "C" {
    fn alchemy_context_switch(old_rsp: *mut u64, new_rsp: u64);
    fn alchemy_jump_to(new_rsp: u64) -> !;
}

fn push_u64(stack_top: u64, value: u64) -> u64 {
    let new_top = (stack_top - 8) & !0x7;
    unsafe {
        (new_top as *mut u64).write(value);
    }
    new_top
}

fn build_initial_stack(stack_top: u64, entry_point: extern "C" fn() -> !) -> u64 {
    let mut rsp = stack_top & !0xF;

    rsp = push_u64(rsp, 0);
    rsp = push_u64(rsp, entry_point as usize as u64);
    rsp = push_u64(rsp, 0);
    rsp = push_u64(rsp, 0);
    rsp = push_u64(rsp, 0);
    rsp = push_u64(rsp, 0);
    rsp = push_u64(rsp, 0);
    rsp = push_u64(rsp, 0);

    rsp
}

extern "C" fn actor_bootstrap() -> ! {
    let entry = {
        let sched = SCHEDULER.lock();
        let idx = sched.current.expect("actor_bootstrap without current actor");
        sched.actors[idx].entry
    };

    entry();
    actor_exited()
}

fn actor_exited() -> ! {
    let target_rsp = {
        let mut sched = SCHEDULER.lock();
        sched.reap_terminated();

        let current_idx = sched.current.expect("actor_exited without current actor");
        sched.actors[current_idx].state = ActorState::Dead;

        match sched.pick_next_ready(Some(current_idx)) {
            Some(next_idx) => {
                sched.actors[next_idx].state = ActorState::Running;
                sched.current = Some(next_idx);
                sched.actors[next_idx].context.rsp
            }
            None => {
                sched.current = None;
                sched.scheduler_rsp
            }
        }
    };

    unsafe { alchemy_jump_to(target_rsp) }
}

pub fn spawn(entry: ActorEntry, supervisor: Option<ActorId>) -> ActorId {
    let id = NEXT_ACTOR_ID.fetch_add(1, Ordering::SeqCst);

    let stack = ActorStack::new(DEFAULT_STACK_PAGES);
    let stack_top = stack.top();
    let rsp = build_initial_stack(stack_top, actor_bootstrap);

    let actor = Actor {
        id,
        supervisor,
        state: ActorState::Ready,
        context: ActorContext { rsp },
        stack: Some(stack),
        mailbox: VecDeque::new(),
        entry,
        cleaned: false,
    };

    let mut sched = SCHEDULER.lock();
    sched.actors.push(actor);

    id
}

pub fn current_actor_id() -> Option<ActorId> {
    let sched = SCHEDULER.lock();
    sched.current.map(|idx| sched.actors[idx].id)
}

pub fn send(to: ActorId, msg: Message) -> bool {
    let mut sched = SCHEDULER.lock();

    let Some(idx) = sched.actor_index_by_id(to) else {
        return false;
    };

    match sched.actors[idx].state {
        ActorState::Dead | ActorState::Crashed => return false,
        _ => {}
    }

    sched.actors[idx].mailbox.push_back(msg);

    if sched.actors[idx].state == ActorState::Waiting {
        sched.actors[idx].state = ActorState::Ready;
    }

    true
}

pub fn receive() -> Message {
    loop {
        {
            let mut sched = SCHEDULER.lock();
            let idx = sched.current.expect("receive called with no current actor");

            if let Some(msg) = sched.actors[idx].mailbox.pop_front() {
                return msg;
            }

            sched.actors[idx].state = ActorState::Waiting;
        }

        yield_internal(false);
    }
}

pub fn yield_now() {
    {
        let mut sched = SCHEDULER.lock();
        let idx = sched.current.expect("yield_now called with no current actor");

        if sched.actors[idx].state == ActorState::Running {
            sched.actors[idx].state = ActorState::Ready;
        }
    }

    yield_internal(false);
}

fn yield_internal(crashed: bool) {
    let (old_rsp_ptr, new_rsp) = {
        let mut sched = SCHEDULER.lock();
        sched.reap_terminated();

        let current_idx = sched.current.expect("yield_internal without current actor");

        if crashed {
        }

        let old_rsp_ptr = &mut sched.actors[current_idx].context.rsp as *mut u64;

        match sched.pick_next_ready(Some(current_idx)) {
            Some(next_idx) => {
                sched.actors[next_idx].state = ActorState::Running;
                sched.current = Some(next_idx);
                (old_rsp_ptr, sched.actors[next_idx].context.rsp)
            }
            None => {
                sched.current = None;
                (old_rsp_ptr, sched.scheduler_rsp)
            }
        }
    };

    unsafe { alchemy_context_switch(old_rsp_ptr, new_rsp) }
}

pub fn run() -> ! {
    loop {
        let maybe_switch = {
            let mut sched = SCHEDULER.lock();
            sched.reap_terminated();

            match sched.pick_next_ready(None) {
                Some(next_idx) => {
                    let sched_rsp_ptr = &mut sched.scheduler_rsp as *mut u64;
                    sched.actors[next_idx].state = ActorState::Running;
                    sched.current = Some(next_idx);
                    Some((sched_rsp_ptr, sched.actors[next_idx].context.rsp))
                }
                None => None,
            }
        };

        match maybe_switch {
            Some((sched_rsp_ptr, next_rsp)) => unsafe {
                alchemy_context_switch(sched_rsp_ptr, next_rsp);
            },
            None => {
                println!("scheduler idle");
                hlt_loop();
            }
        }
    }
}

pub fn crash_current_from_exception(reason: CrashReason) -> ! {
    let target_rsp = {
        let mut sched = SCHEDULER.lock();
        sched.reap_terminated();

        let Some(current_idx) = sched.current else {
            println!("kernel exception outside actor: {:?}", reason);
            hlt_loop();
        };

        let child_id = sched.actors[current_idx].id;
        let supervisor = sched.actors[current_idx].supervisor;
        sched.actors[current_idx].state = ActorState::Crashed;

        if let Some(supervisor_id) = supervisor {
            if let Some(supervisor_idx) = sched.actor_index_by_id(supervisor_id) {
                let parent = &mut sched.actors[supervisor_idx];
                parent.mailbox.push_back(Message::ChildCrashed {
                    child: child_id,
                    reason,
                });

                if parent.state == ActorState::Waiting {
                    parent.state = ActorState::Ready;
                }
            }
        }

        match sched.pick_next_ready(Some(current_idx)) {
            Some(next_idx) => {
                sched.actors[next_idx].state = ActorState::Running;
                sched.current = Some(next_idx);
                sched.actors[next_idx].context.rsp
            }
            None => {
                sched.current = None;
                sched.scheduler_rsp
            }
        }
    };

    unsafe { alchemy_jump_to(target_rsp) }
}

// todo: hook in - timer irq should call this once we add PIT/APIC, for time being can just do cooperative only
pub fn tick() {
    // empty
}
