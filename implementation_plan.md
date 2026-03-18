# Implementation Plan: Zero-Copy "Suspend to Storage" (ix.dev Prototype)

## Goal Description
Build a highly optimized, Linux-first Rust prototype that demonstrates the ability to "freeze" an executing KVM virtual machine, extract its complete state (vCPU registers and guest memory), and sequentially flush it to disk via a zero-copy mechanism using `io_uring`. Finally, demonstrate the ability to identically restore ("wake up") that state.
This solves a key economic and technical hurdle in AI infrastructure: avoiding expensive compute burn while waiting for external model API responses, which closely aligns with **ix.dev's core value proposition of sub-millisecond environment snapshotting**.

## System Architecture mapped to ix.dev constraints
1. **Hypervisor (VMM):** Instead of modifying their proprietary hypervisor, we will utilize the `kvm-ioctls` and `kvm-bindings` crates to interact natively with Linux KVM to build a micro-VMM.
2. **Storage Subsystem:** We will bypass standard userspace buffering entirely. Using the `io-uring` crate (`IORING_OP_WRITE_FIXED` / DMA), guest memory pages will be submitted directly to block storage. This mimics how ix.dev writes directly to their custom libfabric/RDMA content-addressable storage fabric.
3. **Reproducibility:** The project will be a standard `cargo` project with strict `clippy` configurations to align with their "anti-tech-debt" philosophy.

## Proposed Changes / Component Breakdown

### 1. Minimal VMM Setup
- **[NEW] `src/vmm.rs`:** Code to initialize `/dev/kvm`, create a Virtual Machine file descriptor (`VmFd`), and allocate guest RAM explicitly using `mmap`.
- **[NEW] `src/payload.asm` (or equivalent inline payload):** A deterministic, bare-metal 16-bit real-mode infinite loop payload designed to constantly manipulate registers. This proves the state is actually changing before suspension.

### 2. State Extraction (Suspend)
- **[NEW] `src/state.rs`:** Uses KVM `ioctl` calls to pause vCPU execution (`KVM_GET_REGS` and `KVM_GET_SREGS`).
- Serializes the specific architecture states and defines the boundaries of the `guest_memory` array for extraction.

### 3. Storage I/O (io_uring)
- **[NEW] `src/storage.rs`:** Instantiates an `io_uring` instance.
- Creates submission queue entries pointing to the VMM's `mmap`'ed physical memory.
- Triggers an asynchronous flush directly to a state file (`.snapshot`).

### 4. Restoration (Wake Up)
- **[NEW] `src/restore.rs`:** Inverts the suspend logic. Reads the snapshot back into memory via `io_uring` (`IORING_OP_READ_FIXED`), restores the vCPU registers (`KVM_SET_REGS`), and resumes KVM execution, validating that the instructions pick up exactly where they left off.

## Verification Plan
### Automated Testing
- Measure the microsecond-latency breakdown for each layer:
  - Time to halt KVM execution.
  - Time to read registers.
  - Time to perform the complete memory flush to disk using `io_uring`.
- Add an end-to-end integration test asserting the VM completes its dummy payload operations both with and without an intermittent snapshot/resume cycle.

### Manual Verification
- Output rich logs indicating exact memory addresses and register values pre-suspend and post-resume to verify data integrity to visual reviewers.
