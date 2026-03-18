use kvm_bindings::kvm_userspace_memory_region;
use kvm_ioctls::{Kvm, VcpuFd, VmFd};
use std::fs::File;
use std::io::Read;
use std::ptr;

pub const GUEST_MEMORY_SIZE: usize = 0x10000; // 64KB
pub const GUEST_PAYLOAD_OFFSET: usize = 0x1000; // 4KB — payload loaded at 0x1000

/// Holds all VMM state: the KVM handles, guest memory pointer, and vCPU.
pub struct Vmm {
    pub kvm: Kvm,
    pub vm: VmFd,
    pub vcpu: VcpuFd,
    /// Raw pointer to the mmap'd guest memory region.
    pub guest_mem: *mut u8,
    pub guest_mem_size: usize,
}

impl Vmm {
    /// Creates a new micro-VMM:
    ///  1. Opens /dev/kvm
    ///  2. Creates a VM
    ///  3. mmap's guest physical memory
    ///  4. Registers the memory region with KVM
    ///  5. Loads the payload binary into guest memory
    ///  6. Creates a vCPU and sets its initial instruction pointer
    pub fn new(payload_path: &str) -> Self {
        // --- KVM init ---
        let kvm = Kvm::new().expect("Failed to open /dev/kvm");
        let vm = kvm.create_vm().expect("Failed to create VM");

        // --- Allocate guest memory via mmap (zero-copy friendly) ---
        let guest_mem = unsafe {
            libc::mmap(
                ptr::null_mut(),
                GUEST_MEMORY_SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
                -1,
                0,
            )
        };
        if guest_mem == libc::MAP_FAILED {
            panic!("mmap failed for guest memory");
        }
        let guest_mem = guest_mem as *mut u8;

        // --- Register memory region with KVM ---
        let mem_region = kvm_userspace_memory_region {
            slot: 0,
            guest_phys_addr: 0,
            memory_size: GUEST_MEMORY_SIZE as u64,
            userspace_addr: guest_mem as u64,
            flags: 0,
        };
        unsafe {
            vm.set_user_memory_region(mem_region)
                .expect("Failed to set user memory region");
        }

        // --- Load payload ---
        let mut f = File::open(payload_path).expect("Failed to open payload binary");
        let mut payload = Vec::new();
        f.read_to_end(&mut payload).expect("Failed to read payload");

        assert!(
            payload.len() <= GUEST_MEMORY_SIZE - GUEST_PAYLOAD_OFFSET,
            "Payload too large for guest memory"
        );

        unsafe {
            let dest = guest_mem.add(GUEST_PAYLOAD_OFFSET);
            ptr::copy_nonoverlapping(payload.as_ptr(), dest, payload.len());
        }
        println!(
            "[vmm] Loaded {} bytes of payload at guest offset 0x{:X}",
            payload.len(),
            GUEST_PAYLOAD_OFFSET
        );

        // --- Create vCPU ---
        let vcpu = vm.create_vcpu(0).expect("Failed to create vCPU");

        // --- Set initial registers: CS:IP → 0x0000:0x1000 ---
        let mut sregs = vcpu.get_sregs().expect("Failed to get sregs");
        sregs.cs.base = 0;
        sregs.cs.selector = 0;
        vcpu.set_sregs(&sregs).expect("Failed to set sregs");

        let mut regs = vcpu.get_regs().expect("Failed to get regs");
        regs.rip = GUEST_PAYLOAD_OFFSET as u64;
        regs.rflags = 0x2; // reserved bit must be set
        vcpu.set_regs(&regs).expect("Failed to set regs");

        println!("[vmm] vCPU created — RIP = 0x{:X}", regs.rip);

        Vmm {
            kvm,
            vm,
            vcpu,
            guest_mem,
            guest_mem_size: GUEST_MEMORY_SIZE,
        }
    }

    /// Returns an immutable slice over the entire guest physical memory.
    pub fn guest_memory_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.guest_mem, self.guest_mem_size) }
    }

    /// Returns a mutable slice over the entire guest physical memory.
    pub fn guest_memory_slice_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.guest_mem, self.guest_mem_size) }
    }
}

impl Drop for Vmm {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.guest_mem as *mut libc::c_void, self.guest_mem_size);
        }
        println!("[vmm] Guest memory unmapped");
    }
}
