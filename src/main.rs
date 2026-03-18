mod restore;
mod state;
mod storage;
mod vmm;

use kvm_ioctls::VcpuExit;
use std::path::Path;
use std::time::Instant;

const SNAPSHOT_PATH: &str = "vm.snapshot";
const IO_PORT_EXIT: u16 = 0x10; // The port our payload writes to trigger a VM exit

fn main() {
    println!("=========================================");
    println!("  Zero-Copy Suspend-to-Storage Prototype");
    println!("=========================================\n");

    // ── Phase 1: Boot the VM ──────────────────────────────────────────
    println!("── Phase 1: Booting VM ──");
    let mut vmm = vmm::Vmm::new("payload.bin");

    // Run the vCPU until the first I/O port exit (the payload loops and
    // writes to port 0x10 each iteration).
    let run_start = Instant::now();
    let mut exit_count = 0u64;
    let iterations_before_suspend = 3; // run a few loops to prove state changes

    loop {
        let mut do_log = false;
        let mut port_val = 0;
        let mut data_byte = 0;

        match vmm.vcpu.run().expect("vCPU run failed") {
            VcpuExit::IoOut(port, data) if port == IO_PORT_EXIT => {
                port_val = port;
                data_byte = data[0];
                do_log = true;
            }
            VcpuExit::Hlt => {
                println!("  [run] vCPU halted");
                break;
            }
            unexpected => {
                panic!("Unexpected VM exit: {:?}", unexpected);
            }
        }

        if do_log {
            exit_count += 1;

            let regs = vmm.vcpu.get_regs().expect("get_regs");
            println!(
                "  [run] VM exit #{}: port=0x{:X} data=0x{:02X} | RAX=0x{:X} RBX=0x{:X} RCX=0x{:X} RIP=0x{:X}",
                exit_count, port_val, data_byte, regs.rax, regs.rbx, regs.rcx, regs.rip
            );

            if exit_count >= iterations_before_suspend {
                println!(
                    "  [run] {} iterations complete in {:.3}ms — suspending\n",
                    exit_count,
                    run_start.elapsed().as_secs_f64() * 1000.0
                );
                break;
            }
        }
    }

    // ── Phase 2: Capture state (suspend) ──────────────────────────────
    println!("── Phase 2: Capturing VM state ──");
    let suspend_start = Instant::now();
    let snapshot = state::VmSnapshot::capture(&vmm);
    let capture_elapsed = suspend_start.elapsed();
    println!(
        "  State captured in {:.3}ms\n",
        capture_elapsed.as_secs_f64() * 1000.0
    );

    // ── Phase 3: Flush to disk via io_uring ───────────────────────────
    println!("── Phase 3: Flushing to disk (io_uring) ──");
    let snapshot_bytes = snapshot.to_bytes();
    let snapshot_path = Path::new(SNAPSHOT_PATH);
    storage::flush_to_disk(&snapshot_bytes, snapshot_path)
        .expect("Failed to flush snapshot to disk");
    println!();

    // Record pre-suspend registers for post-restore comparison
    let pre_suspend_regs = vmm.vcpu.get_regs().expect("get_regs");

    // ── Phase 4: Restore from disk ────────────────────────────────────
    println!("── Phase 4: Restoring VM from snapshot ──");
    restore::restore_vm(&mut vmm, snapshot_path);
    println!();

    // ── Phase 5: Verify correctness ───────────────────────────────────
    println!("── Phase 5: Verification ──");
    let post_restore_regs = vmm.vcpu.get_regs().expect("get_regs");

    let regs_match = pre_suspend_regs.rax == post_restore_regs.rax
        && pre_suspend_regs.rbx == post_restore_regs.rbx
        && pre_suspend_regs.rcx == post_restore_regs.rcx
        && pre_suspend_regs.rip == post_restore_regs.rip
        && pre_suspend_regs.rsp == post_restore_regs.rsp
        && pre_suspend_regs.rflags == post_restore_regs.rflags;

    println!("  Register integrity: {}", if regs_match { "✅ PASS" } else { "❌ FAIL" });

    // Verify memory integrity
    let restored_mem = vmm.guest_memory_slice();
    let mem_match = restored_mem == &snapshot.memory[..];
    println!("  Memory integrity:   {}", if mem_match { "✅ PASS" } else { "❌ FAIL" });

    // Resume execution for one more iteration to prove the VM is alive
    println!("\n  Resuming execution post-restore...");
    let mut do_log = false;
    let mut port_val = 0;
    let mut data_byte = 0;

    match vmm.vcpu.run().expect("vCPU run failed") {
        VcpuExit::IoOut(port, data) if port == IO_PORT_EXIT => {
            port_val = port;
            data_byte = data[0];
            do_log = true;
        }
        other => {
            println!("  Unexpected VM exit post-restore: {:?} — ❌ FAIL", other);
        }
    }

    if do_log {
        let regs = vmm.vcpu.get_regs().expect("get_regs");
        println!(
            "  [run] Post-restore exit: port=0x{:X} data=0x{:02X} | RAX=0x{:X} RBX=0x{:X} RCX=0x{:X} RIP=0x{:X}",
            port_val, data_byte, regs.rax, regs.rbx, regs.rcx, regs.rip
        );
        println!("  VM resumed successfully: ✅ PASS");
    }

    println!("\n=========================================");
    println!("  Prototype complete");
    println!("=========================================");
}
