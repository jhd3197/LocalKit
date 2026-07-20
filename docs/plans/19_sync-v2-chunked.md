# 19 — Sync v2: chunked transfers, byte progress, resume, cancel

Status: ⬜ planned

Replace the monolithic in-memory push/pull with a chunked, resumable
transfer protocol between LocalKit and the `serverkit-localkit` extension,
with real byte-level progress and cancellable operations.

## Motivation

Sync v1 (plan 4) builds the whole `wp-content` tar.gz in memory, POSTs it in
one request, and hopes: bounded by the server's 100 MB body limit, no
progress beyond coarse stages, a dropped connection at 99% means starting
over, and the UI can offer no cancel button because the operation is one
giant `await`. Any site with a real `uploads/` directory hits these walls.
The extension's own docstring already flags "sync runs inline, no job queue"
as its known v1 limitation.

## Design

### Phase 1 — Chunked upload protocol (both sides)

- Server (`serverkit-localkit` extension, ServerKit repo):
  - `POST /push/{code,db}/init` → `{transfer_id}`; body describes the
    transfer: `site_id`, `total_bytes`, `chunk_size`, `sha256` of the whole
    archive, plus operation metadata (`local_url` for DB pushes).
  - `PUT /push/{code,db}/chunk` — `{transfer_id, offset, sha256}` + raw
    body; server writes the range into a temp file, records the chunk hash,
    returns the set of offsets already confirmed (idempotent: re-sending a
    confirmed chunk is a no-op 200).
  - `POST /push/{code,db}/finish` — verifies whole-file sha256, then runs
    the *existing* v1 processing path (safe-extract → `docker cp` / DB
    import → search-replace) on the assembled temp file, streams the result.
  - Stale transfers (no chunk for 30 min) are reaped by a lazy sweep on
    `init`.
- Client (`src-tauri/src/sync.rs`): stream the archive through a
  hasher+chunker (8 MiB chunks) instead of a `Vec<u8>` — for push code this
  also means tarring straight to the socket pipeline instead of memory,
  which fixes the RAM blowup on big sites as a side effect.
- Resume: before uploading, `init` returns any previously confirmed offsets
  for the same `(site_id, sha256)`; the client skips those chunks. Retrying
  a failed push therefore re-sends only what was lost.

### Phase 2 — Progress, cancel, versioning

- `site-event` payload extended with `{bytes_done, bytes_total}` during
  transfer stages; `sites.ts handleEvent` renders "Pushing code —
  148 / 312 MB" in the pinned progress toast (keep stage messages for the
  non-transfer phases).
- Cancel: new `cancel_sync(site_id)` command drops an `Arc<AtomicBool>`
  checked between chunks; server-side, an unfinished transfer is simply
  abandoned and reaped — no half-applied state is possible because
  processing only happens in `finish` after hash verification.
- Capability negotiation: `GET /pair` `features` array (plan 18) gains
  `"sync-v2"`. Older extension → LocalKit silently uses the v1 monolithic
  path with the v1 progress granularity. One client, both servers.

### Phase 3 — Pull direction + job handoff

- Downloads: server sends `Content-Length` and supports `Range`; client
  downloads in ranges to a temp file with the same resume/verify/cancel
  mechanics, then imports. No protocol invention needed — HTTP already is
  the chunked protocol here.
- Server-side long processing (the import/extract in `finish`) moves onto
  the extension's job queue with a `GET /jobs/<id>` poll, so a client
  disconnect during *processing* (not transfer) can re-attach and learn the
  outcome instead of guessing. `SyncRecord` is written from the poll result.
- Timeouts: reqwest per-chunk timeout only (no whole-request cap); the
  whole operation is bounded by liveness, not duration.

## Risks

- Two code paths (v1/v2) in `sync.rs` — keep v1 as a single isolated
  function and route through a `match features` at the top; do not sprinkle
  conditionals through the flow.
- Hash-verified `finish` means the server holds a trusted-but-unprocessed
  archive; the safe-extract policy stays mandatory exactly as in v1.
- Chunk size tradeoff: 8 MiB is a round number that keeps request counts
  low on LAN-ish links without making progress bars jumpy; make it a const,
  not a setting.

## Verification

- `examples/mock_localkit_ext.cjs` implements v2 (in-memory chunk store) →
  `m4_smoke` runs the full v2 flow against it, including: kill the client
  mid-upload, re-run, assert only missing chunks were re-sent (mock counts
  requests) and the final hashes match.
- Synthetic >100 MB `wp-content` fixture (sparse files) proves the v1 limit
  is gone and memory stays flat (`docker stats`-level eyeball is fine).
- Unit tests: chunker/hasher pipeline (offset math, hash continuity),
  resume-set subtraction.
