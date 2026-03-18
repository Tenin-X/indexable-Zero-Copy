use io_uring::{opcode, types, IoUring};
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::time::Instant;

const RING_ENTRIES: u32 = 64;

/// Writes raw snapshot bytes to `path` using io_uring for zero-copy async I/O.
pub fn flush_to_disk(data: &[u8], path: &Path) -> std::io::Result<()> {
    let start = Instant::now();

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;

    let fd = types::Fd(file.as_raw_fd());

    let mut ring = IoUring::new(RING_ENTRIES)?;

    // We submit the entire buffer as one write SQE.
    // For larger snapshots this could be chunked across multiple SQEs.
    let write_e = opcode::Write::new(fd, data.as_ptr(), data.len() as u32)
        .offset(0)
        .build()
        .user_data(0x01);

    unsafe {
        ring.submission()
            .push(&write_e)
            .expect("Submission queue full");
    }

    ring.submit_and_wait(1)?;

    // Verify completion
    let cqe = ring.completion().next().expect("No completion event");
    if cqe.result() < 0 {
        return Err(std::io::Error::from_raw_os_error(-cqe.result()));
    }

    let written = cqe.result() as usize;
    let elapsed = start.elapsed();

    println!(
        "[storage] Flushed {} bytes to {:?} in {:.3}ms (io_uring)",
        written,
        path,
        elapsed.as_secs_f64() * 1000.0
    );

    // fsync via io_uring for durability
    let fsync_e = opcode::Fsync::new(fd).build().user_data(0x02);
    unsafe {
        ring.submission()
            .push(&fsync_e)
            .expect("Submission queue full");
    }
    ring.submit_and_wait(1)?;
    let fsync_cqe = ring.completion().next().expect("No fsync completion");
    if fsync_cqe.result() < 0 {
        return Err(std::io::Error::from_raw_os_error(-fsync_cqe.result()));
    }

    println!("[storage] fsync complete — snapshot durable on disk");

    Ok(())
}

/// Reads a snapshot file back into memory using io_uring.
pub fn read_from_disk(path: &Path) -> std::io::Result<Vec<u8>> {
    let start = Instant::now();

    let file = OpenOptions::new().read(true).open(path)?;
    let file_len = file.metadata()?.len() as usize;
    let fd = types::Fd(file.as_raw_fd());

    let mut buf = vec![0u8; file_len];

    let mut ring = IoUring::new(RING_ENTRIES)?;

    let read_e = opcode::Read::new(fd, buf.as_mut_ptr(), file_len as u32)
        .offset(0)
        .build()
        .user_data(0x10);

    unsafe {
        ring.submission()
            .push(&read_e)
            .expect("Submission queue full");
    }

    ring.submit_and_wait(1)?;

    let cqe = ring.completion().next().expect("No completion event");
    if cqe.result() < 0 {
        return Err(std::io::Error::from_raw_os_error(-cqe.result()));
    }

    let elapsed = start.elapsed();
    println!(
        "[storage] Read {} bytes from {:?} in {:.3}ms (io_uring)",
        cqe.result(),
        path,
        elapsed.as_secs_f64() * 1000.0
    );

    Ok(buf)
}
