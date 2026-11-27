//! A big copy of the player's game (a folder of tens of GB, a disc unpacked)
//! says how far it is and can be stopped. The copy routines count what they
//! write here and look at the stop mark between blocks; the command that
//! started the copy reads the count to show progress.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static COPIED: AtomicU64 = AtomicU64::new(0);
static STOP: AtomicBool = AtomicBool::new(false);
/// A copy that shows progress is going on: only then does Cancelar stop anything.
static ACTIVE: AtomicBool = AtomicBool::new(false);

// Tests run side by side: each keeps its own count and stop mark.
#[cfg(test)]
thread_local! {
    static TEST_STOP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn stopped() -> bool {
    #[cfg(test)]
    return TEST_STOP.with(std::cell::Cell::get);
    #[cfg(not(test))]
    STOP.load(Ordering::Relaxed)
}

/// The word an interrupted copy's error carries.
pub const STOPPED: &str = "copy stopped";

/// A new copy begins: nothing counted, not stopped.
pub fn begin() {
    COPIED.store(0, Ordering::SeqCst);
    STOP.store(false, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    #[cfg(test)]
    TEST_STOP.with(|s| s.set(false));
}

/// The copy ended: a later copy (moving a game, Easy Setup) is never stopped by an old Cancelar.
pub fn finish() {
    ACTIVE.store(false, Ordering::SeqCst);
    STOP.store(false, Ordering::SeqCst);
}

/// Bytes written since `begin`.
pub fn copied() -> u64 {
    COPIED.load(Ordering::Relaxed)
}

/// The player asked to stop.
pub fn stop() {
    if ACTIVE.load(Ordering::SeqCst) {
        STOP.store(true, Ordering::SeqCst);
    }
    #[cfg(test)]
    TEST_STOP.with(|s| s.set(true));
}

/// Counts `n` bytes written; an error when the player asked to stop.
pub fn tick(n: u64) -> io::Result<()> {
    COPIED.fetch_add(n, Ordering::Relaxed);
    if stopped() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, STOPPED));
    }
    Ok(())
}

/// Whether an error is the player's stop.
pub fn was_stopped(e: &dyn std::fmt::Display) -> bool {
    e.to_string().contains(STOPPED)
}

/// `std::io::copy` in blocks, counted and stoppable.
pub fn stream(from: &mut dyn Read, to: &mut dyn Write) -> io::Result<u64> {
    let mut buffer = vec![0u8; 1 << 20];
    let mut total = 0;
    loop {
        let n = match from.read(&mut buffer) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted && !stopped() => continue,
            Err(e) => return Err(e),
        };
        to.write_all(&buffer[..n])?;
        total += n as u64;
        tick(n as u64)?;
    }
}

/// `std::fs::copy`, counted and stoppable.
pub fn file(from: &Path, to: &Path) -> io::Result<u64> {
    let mut source = File::open(from)?;
    let mut target = File::create(to)?;
    let copied = stream(&mut source, &mut target)?;
    target.flush()?;
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_is_counted_and_stops_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("big.bin");
        std::fs::write(&from, vec![7u8; 3 << 20]).unwrap();
        begin();
        assert_eq!(file(&from, &dir.path().join("a.bin")).unwrap(), 3 << 20);
        begin();
        stop();
        let error = file(&from, &dir.path().join("b.bin")).unwrap_err();
        assert!(was_stopped(&error));
        begin();
    }
}
