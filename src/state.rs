use kvm_bindings::{kvm_regs, kvm_sregs};
use std::io::Write;

use crate::vmm::{Vmm, GUEST_MEMORY_SIZE};

/// Complete snapshot of a suspended VM: CPU registers + guest RAM.
pub struct VmSnapshot {
    pub regs: kvm_regs,
    pub sregs: kvm_sregs,
    pub memory: Vec<u8>,
}

impl VmSnapshot {
    /// Captures the full VM state: general-purpose registers, segment registers,
    /// and the entire guest physical memory.
    pub fn capture(vmm: &Vmm) -> Self {
        let regs = vmm.vcpu.get_regs().expect("Failed to KVM_GET_REGS");
        let sregs = vmm.vcpu.get_sregs().expect("Failed to KVM_GET_SREGS");
        let memory = vmm.guest_memory_slice().to_vec();

        println!("[state] Captured VM snapshot:");
        println!(
            "  regs: RAX=0x{:X}  RBX=0x{:X}  RCX=0x{:X}  RIP=0x{:X}",
            regs.rax, regs.rbx, regs.rcx, regs.rip
        );
        println!("  memory: {} bytes", memory.len());

        VmSnapshot {
            regs,
            sregs,
            memory,
        }
    }

    /// Serializes the snapshot into a flat byte buffer:
    ///   [ kvm_regs bytes | kvm_sregs bytes | guest memory ]
    pub fn to_bytes(&self) -> Vec<u8> {
        let regs_size = std::mem::size_of::<kvm_regs>();
        let sregs_size = std::mem::size_of::<kvm_sregs>();
        let total = regs_size + sregs_size + self.memory.len();
        let mut buf = Vec::with_capacity(total);

        // Safety: kvm_regs / kvm_sregs are plain-old-data structs
        let regs_bytes =
            unsafe { std::slice::from_raw_parts(&self.regs as *const _ as *const u8, regs_size) };
        let sregs_bytes = unsafe {
            std::slice::from_raw_parts(&self.sregs as *const _ as *const u8, sregs_size)
        };

        buf.write_all(regs_bytes).unwrap();
        buf.write_all(sregs_bytes).unwrap();
        buf.write_all(&self.memory).unwrap();

        buf
    }

    /// Deserializes a snapshot from a flat byte buffer.
    pub fn from_bytes(data: &[u8]) -> Self {
        let regs_size = std::mem::size_of::<kvm_regs>();
        let sregs_size = std::mem::size_of::<kvm_sregs>();
        let header_size = regs_size + sregs_size;

        assert!(
            data.len() >= header_size + GUEST_MEMORY_SIZE,
            "Snapshot data too small: {} bytes (need at least {})",
            data.len(),
            header_size + GUEST_MEMORY_SIZE
        );

        let regs: kvm_regs = unsafe { std::ptr::read(data.as_ptr() as *const kvm_regs) };
        let sregs: kvm_sregs =
            unsafe { std::ptr::read(data[regs_size..].as_ptr() as *const kvm_sregs) };
        let memory = data[header_size..header_size + GUEST_MEMORY_SIZE].to_vec();

        println!("[state] Deserialized snapshot:");
        println!(
            "  regs: RAX=0x{:X}  RBX=0x{:X}  RCX=0x{:X}  RIP=0x{:X}",
            regs.rax, regs.rbx, regs.rcx, regs.rip
        );
        println!("  memory: {} bytes", memory.len());

        VmSnapshot {
            regs,
            sregs,
            memory,
        }
    }
}
