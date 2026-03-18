use crate::state::VmSnapshot;
use crate::storage;
use crate::vmm::Vmm;
use std::path::Path;
use std::time::Instant;

/// Restores a VM from a `.snapshot` file on disk:
///  1. Reads the snapshot via io_uring
///  2. Deserializes registers + memory
///  3. Writes everything back into the VMM's live state
///  4. The caller can then resume vCPU execution
pub fn restore_vm(vmm: &mut Vmm, snapshot_path: &Path) {
    let start = Instant::now();

    // 1. Read snapshot from disk via io_uring
    let data = storage::read_from_disk(snapshot_path)
        .expect("Failed to read snapshot from disk");

    // 2. Deserialize
    let snapshot = VmSnapshot::from_bytes(&data);

    // 3. Restore vCPU registers
    vmm.vcpu
        .set_regs(&snapshot.regs)
        .expect("Failed to KVM_SET_REGS");
    vmm.vcpu
        .set_sregs(&snapshot.sregs)
        .expect("Failed to KVM_SET_SREGS");

    // 4. Restore guest memory
    let guest_mem = vmm.guest_memory_slice_mut();
    guest_mem.copy_from_slice(&snapshot.memory);

    let elapsed = start.elapsed();
    println!(
        "[restore] VM state fully restored in {:.3}ms",
        elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "[restore] Resumed at RIP=0x{:X}  RAX=0x{:X}  RBX=0x{:X}  RCX=0x{:X}",
        snapshot.regs.rip, snapshot.regs.rax, snapshot.regs.rbx, snapshot.regs.rcx
    );
}
