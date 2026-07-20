//! Chunked, resumable transfer primitives (plan 19).
//!
//! Sync v1 built the whole payload in a `Vec<u8>`, POSTed it in one request and
//! hoped: bounded by the server's 100 MB body limit, no progress beyond coarse
//! stages, and a dropped connection at 99% meant starting over. This module is
//! the substrate that replaces it — everything here is deliberately pure or
//! filesystem-only, with no HTTP in sight, so the offset math and the resume
//! rule can be unit-tested without a server.
//!
//! The shape of a v2 transfer:
//!
//! 1. **Stage** the payload to a temp file, hashing it in the same pass
//!    (`stage` / `adopt`) — a `Staged` deletes itself on drop, so no failure
//!    path leaks a multi-hundred-MB file into the temp dir.
//! 2. **Plan** the upload as `Chunk`s, subtracting whatever the server already
//!    confirmed (`remaining`) — that subtraction *is* resume.
//! 3. **Send** each chunk, checking a `CancelToken` between them.
//!
//! Chunk size is a const, not a setting: 8 MiB keeps request counts low on
//! LAN-ish links without making the progress bar jumpy, and a knob here would
//! only ever be turned to a worse value.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

/// Bytes per chunk. See the module docs for why this is not configurable.
pub const CHUNK_SIZE: u64 = 8 * 1024 * 1024;

/// Error text a cancelled transfer fails with. Callers compare against this
/// (`is_cancel`) to report "cancelled" instead of "failed" — a user pressing
/// Cancel is not an error condition, it just travels the error path.
pub const CANCELLED: &str = "cancelled";

/// Was this error a user cancel rather than a real failure?
pub fn is_cancel(e: &str) -> bool {
    e == CANCELLED
}

// ---------------------------------------------------------------------------
// Chunk planning
// ---------------------------------------------------------------------------

/// One byte range of a staged payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    pub offset: u64,
    pub len: u64,
}

impl Chunk {
    pub fn end(&self) -> u64 {
        self.offset + self.len
    }
}

/// Every chunk of a `total`-byte payload, in order.
///
/// A zero-byte payload yields no chunks — `finish` still runs, and the server
/// verifies the hash of nothing against the hash of nothing. That is the
/// correct behavior for an empty archive even though the callers above refuse
/// to produce one.
pub fn chunks(total: u64, chunk_size: u64) -> Vec<Chunk> {
    assert!(chunk_size > 0, "chunk_size must be positive");
    let mut out = Vec::new();
    let mut offset = 0u64;
    while offset < total {
        out.push(Chunk {
            offset,
            // The last chunk is short; every other one is full.
            len: chunk_size.min(total - offset),
        });
        offset += chunk_size;
    }
    out
}

/// The chunks still to send, given the offsets the server says it already has.
///
/// This subtraction is the whole of resume: `init` reports what survived a
/// previous attempt, and the client simply skips those. Unknown offsets in
/// `confirmed` (a server confirming something we never sent, or a chunk size
/// that changed between attempts) are ignored rather than trusted — the plan
/// is always derived from *our* view of the payload.
pub fn remaining(total: u64, chunk_size: u64, confirmed: &[u64]) -> Vec<Chunk> {
    chunks(total, chunk_size)
        .into_iter()
        .filter(|c| !confirmed.contains(&c.offset))
        .collect()
}

/// Total bytes covered by a chunk list — what "already done" means for the
/// progress readout when a resumed transfer starts part-way through.
pub fn bytes_of(chunks: &[Chunk]) -> u64 {
    chunks.iter().map(|c| c.len).sum()
}

// ---------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------

/// A writer that hashes everything passing through it.
///
/// Used so a payload is hashed *while* it is built rather than in a second
/// read pass — for a multi-GB `wp-content` archive that halves the disk IO and
/// keeps peak memory at one buffer.
pub struct HashWriter<W: Write> {
    inner: W,
    hasher: Sha256,
    written: u64,
}

impl<W: Write> HashWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner, hasher: Sha256::new(), written: 0 }
    }

    /// Consume the writer, returning the wrapped writer, the hex digest and the
    /// byte count.
    pub fn finish(self) -> (W, String, u64) {
        (self.inner, hex(&self.hasher.finalize()), self.written)
    }
}

impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Hash only what the inner writer actually accepted, or a short write
        // would poison the digest with bytes that never reached the file.
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

// ---------------------------------------------------------------------------
// Staged payloads
// ---------------------------------------------------------------------------

/// A payload written to a temp file, with its size and hash.
///
/// The file is removed on drop, including on every error path — a failed push
/// must not leave a copy of the site's `wp-content` sitting in the temp dir.
#[derive(Debug)]
pub struct Staged {
    path: PathBuf,
    total: u64,
    sha256: String,
}

impl Staged {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Read one chunk back out for sending.
    ///
    /// Blocking IO on purpose: a chunk is at most `CHUNK_SIZE` off local disk,
    /// and the rest of this codebase reads files the same way. The alternative
    /// (holding an async file handle across the whole upload) buys nothing and
    /// complicates the resume path.
    pub fn read_chunk(&self, chunk: Chunk) -> Result<Vec<u8>, String> {
        let mut f = std::fs::File::open(&self.path)
            .map_err(|e| format!("failed to reopen the staged payload: {e}"))?;
        f.seek(SeekFrom::Start(chunk.offset))
            .map_err(|e| format!("failed to seek the staged payload: {e}"))?;
        let mut buf = vec![0u8; chunk.len as usize];
        f.read_exact(&mut buf)
            .map_err(|e| format!("failed to read the staged payload: {e}"))?;
        Ok(buf)
    }

    /// Take ownership of a file someone else wrote (e.g. `wp db export`),
    /// hashing it in place. The file is deleted on drop just like a staged one.
    pub fn adopt(path: PathBuf) -> Result<Self, String> {
        let mut f = std::fs::File::open(&path)
            .map_err(|e| format!("failed to open the staged payload: {e}"))?;
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f
                .read(&mut buf)
                .map_err(|e| format!("failed to read the staged payload: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            total += n as u64;
        }
        Ok(Self { path, total, sha256: hex(&hasher.finalize()) })
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Write a payload to a temp file through a hashing writer.
///
/// `tag` only exists to make a stray file identifiable if the process is killed
/// hard enough to skip `Drop`.
pub fn stage<F>(tag: &str, build: F) -> Result<Staged, String>
where
    F: FnOnce(&mut dyn Write) -> Result<(), String>,
{
    let path = std::env::temp_dir().join(format!(
        "localkit-{tag}-{}-{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let file = std::fs::File::create(&path)
        .map_err(|e| format!("failed to create a staging file: {e}"))?;

    // Hash first, buffer second: every byte still passes through the hasher,
    // and the buffer keeps the 8 KiB tar writes off the syscall path.
    let mut writer = HashWriter::new(std::io::BufWriter::new(file));
    let built = build(&mut writer);
    let (mut inner, sha256, total) = writer.finish();
    let flushed = inner
        .flush()
        .map_err(|e| format!("failed to finish the staging file: {e}"));

    // A `Staged` exists from here on, so any error below still cleans up.
    let staged = Staged { path, total, sha256 };
    built?;
    flushed?;
    Ok(staged)
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

/// Per-site cancel flags for in-flight transfers.
///
/// Keyed by site id because that is what the UI has to offer a Cancel button
/// against — one sync per site at a time is already the assumption everywhere
/// else (the pinned progress toast, `busyId`).
#[derive(Clone, Default)]
pub struct CancelRegistry {
    inner: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl CancelRegistry {
    /// Register a cancellable operation for `site_id`.
    ///
    /// Replaces any token left behind by a previous operation, so a stale flag
    /// can never cancel a fresh transfer the moment it starts.
    pub fn begin(&self, site_id: &str) -> CancelToken {
        let flag = Arc::new(AtomicBool::new(false));
        if let Ok(mut map) = self.inner.lock() {
            map.insert(site_id.to_string(), flag.clone());
        }
        CancelToken { site_id: site_id.to_string(), flag, registry: self.clone() }
    }

    /// Ask the transfer for `site_id` to stop. Returns whether one was running.
    pub fn cancel(&self, site_id: &str) -> bool {
        match self.inner.lock() {
            Ok(map) => match map.get(site_id) {
                Some(flag) => {
                    flag.store(true, Ordering::SeqCst);
                    true
                }
                None => false,
            },
            Err(_) => false,
        }
    }
}

/// Handle held by the running transfer; deregisters itself on drop.
pub struct CancelToken {
    site_id: String,
    flag: Arc<AtomicBool>,
    registry: CancelRegistry,
}

impl CancelToken {
    /// `Err(CANCELLED)` once the user has asked to stop. Called between chunks,
    /// which is also the only place a transfer can stop cleanly: a half-sent
    /// chunk is simply never confirmed, and the server reaps the transfer.
    pub fn check(&self) -> Result<(), String> {
        if self.flag.load(Ordering::SeqCst) {
            Err(CANCELLED.to_string())
        } else {
            Ok(())
        }
    }

    pub fn cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

impl Drop for CancelToken {
    fn drop(&mut self) {
        if let Ok(mut map) = self.registry.inner.lock() {
            // Only remove our own token — a newer transfer for the same site
            // may already have replaced it, and dropping that one would leave
            // it uncancellable.
            if map.get(&self.site_id).is_some_and(|f| Arc::ptr_eq(f, &self.flag)) {
                map.remove(&self.site_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- chunk math ---------------------------------------------------------

    #[test]
    fn chunks_cover_the_payload_exactly() {
        for total in [0u64, 1, 9, 10, 11, 99, 100, 101] {
            let cs = 10;
            let plan = chunks(total, cs);
            assert_eq!(bytes_of(&plan), total, "coverage gap at total={total}");
            // Contiguous, in order, no overlaps.
            let mut expected = 0;
            for c in &plan {
                assert_eq!(c.offset, expected, "gap/overlap at total={total}");
                assert!(c.len > 0 && c.len <= cs);
                expected = c.end();
            }
            assert_eq!(expected, total);
        }
    }

    #[test]
    fn the_last_chunk_is_short_and_the_rest_are_full() {
        let plan = chunks(25, 10);
        assert_eq!(
            plan,
            vec![
                Chunk { offset: 0, len: 10 },
                Chunk { offset: 10, len: 10 },
                Chunk { offset: 20, len: 5 },
            ]
        );
    }

    #[test]
    fn an_exact_multiple_produces_no_empty_trailing_chunk() {
        let plan = chunks(20, 10);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan.last().unwrap().len, 10);
    }

    #[test]
    fn an_empty_payload_has_no_chunks() {
        assert!(chunks(0, CHUNK_SIZE).is_empty());
    }

    // -- resume -------------------------------------------------------------

    #[test]
    fn confirmed_offsets_are_skipped() {
        let left = remaining(25, 10, &[0, 20]);
        assert_eq!(left, vec![Chunk { offset: 10, len: 10 }]);
    }

    #[test]
    fn nothing_confirmed_means_everything_is_sent() {
        assert_eq!(remaining(25, 10, &[]), chunks(25, 10));
    }

    #[test]
    fn everything_confirmed_means_nothing_is_sent() {
        assert!(remaining(25, 10, &[0, 10, 20]).is_empty());
    }

    #[test]
    fn offsets_the_client_never_planned_are_ignored() {
        // A server echoing an offset from a differently-chunked attempt must
        // not be able to make us skip a real chunk.
        let left = remaining(25, 10, &[5, 15, 99, 1_000_000]);
        assert_eq!(left, chunks(25, 10), "a bogus offset suppressed a real chunk");
    }

    // -- hashing ------------------------------------------------------------

    #[test]
    fn hash_writer_matches_a_one_shot_hash() {
        let payload: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let mut w = HashWriter::new(Vec::new());
        // Written in uneven pieces: the digest must not depend on write sizes.
        for piece in payload.chunks(777) {
            w.write_all(piece).unwrap();
        }
        let (out, digest, written) = w.finish();
        assert_eq!(out, payload);
        assert_eq!(written, payload.len() as u64);
        assert_eq!(digest, sha256_hex(&payload));
    }

    #[test]
    fn sha256_is_the_known_answer() {
        // Anchors the hex encoding against a value that is not self-generated.
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // -- staging ------------------------------------------------------------

    #[test]
    fn staging_hashes_and_sizes_what_it_wrote() {
        let payload: Vec<u8> = (0..5_000u32).map(|i| (i % 97) as u8).collect();
        let staged = stage("test-stage", |w| {
            w.write_all(&payload).map_err(|e| e.to_string())
        })
        .unwrap();

        assert_eq!(staged.total(), payload.len() as u64);
        assert_eq!(staged.sha256(), sha256_hex(&payload));
        assert_eq!(std::fs::read(staged.path()).unwrap(), payload);
    }

    #[test]
    fn reassembling_the_chunks_reproduces_the_payload() {
        let payload: Vec<u8> = (0..30_000u32).map(|i| (i % 253) as u8).collect();
        let staged = stage("test-chunks", |w| {
            w.write_all(&payload).map_err(|e| e.to_string())
        })
        .unwrap();

        // Send them out of order, exactly as a resumed transfer would.
        let plan = chunks(staged.total(), 4096);
        let mut assembled = vec![0u8; payload.len()];
        for chunk in plan.iter().rev() {
            let bytes = staged.read_chunk(*chunk).unwrap();
            assert_eq!(bytes.len(), chunk.len as usize);
            assembled[chunk.offset as usize..chunk.end() as usize].copy_from_slice(&bytes);
        }
        assert_eq!(assembled, payload);
        assert_eq!(sha256_hex(&assembled), staged.sha256());
    }

    #[test]
    fn a_staged_file_is_removed_on_drop() {
        let path = {
            let staged = stage("test-drop", |w| w.write_all(b"x").map_err(|e| e.to_string())).unwrap();
            staged.path().to_path_buf()
        };
        assert!(!path.exists(), "the staged file outlived its Staged");
    }

    #[test]
    fn a_failed_build_still_cleans_up() {
        let err = stage("test-fail", |w| {
            w.write_all(b"partial").map_err(|e| e.to_string())?;
            Err("bundling blew up".into())
        })
        .unwrap_err();
        assert_eq!(err, "bundling blew up");
        // Nothing to assert on the path (we never got one) beyond: no panic,
        // and the temp dir is not accumulating — covered by the drop test.
    }

    #[test]
    fn adopting_a_file_hashes_it_and_takes_ownership() {
        let path = std::env::temp_dir().join(format!("localkit-adopt-{}.sql", std::process::id()));
        std::fs::write(&path, b"SELECT 1;").unwrap();
        let (total, digest) = {
            let staged = Staged::adopt(path.clone()).unwrap();
            (staged.total(), staged.sha256().to_string())
        };
        assert_eq!(total, 9);
        assert_eq!(digest, sha256_hex(b"SELECT 1;"));
        assert!(!path.exists(), "adopt did not take ownership of the file");
    }

    // -- cancellation -------------------------------------------------------

    #[test]
    fn a_token_reports_cancellation() {
        let reg = CancelRegistry::default();
        let token = reg.begin("site-a");
        assert!(token.check().is_ok());

        assert!(reg.cancel("site-a"), "cancel did not find the running transfer");
        assert!(token.cancelled());
        assert_eq!(token.check().unwrap_err(), CANCELLED);
        assert!(is_cancel(&token.check().unwrap_err()));
    }

    #[test]
    fn cancelling_an_idle_site_is_a_no_op() {
        let reg = CancelRegistry::default();
        assert!(!reg.cancel("nobody-home"));
    }

    #[test]
    fn a_token_deregisters_itself() {
        let reg = CancelRegistry::default();
        drop(reg.begin("site-a"));
        assert!(!reg.cancel("site-a"), "a finished transfer is still cancellable");
    }

    #[test]
    fn cancels_do_not_leak_across_sites() {
        let reg = CancelRegistry::default();
        let a = reg.begin("site-a");
        let b = reg.begin("site-b");
        reg.cancel("site-a");
        assert!(a.cancelled());
        assert!(!b.cancelled(), "cancelling one site stopped another");
    }

    #[test]
    fn a_stale_flag_cannot_cancel_the_next_transfer() {
        let reg = CancelRegistry::default();
        let first = reg.begin("site-a");
        reg.cancel("site-a");
        assert!(first.cancelled());

        // Second run of the same site: fresh flag, and dropping the old token
        // must not deregister the new one.
        let second = reg.begin("site-a");
        drop(first);
        assert!(!second.cancelled(), "a stale flag cancelled a fresh transfer");
        assert!(reg.cancel("site-a"), "the fresh transfer was deregistered by the stale token");
        assert!(second.cancelled());
    }
}
