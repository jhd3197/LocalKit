# 19 — Sync v2: chunked transfers, byte progress, resume, cancel

Status: ✅ shipped (one deferred item — see *What shipped* below)

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

## What shipped

Phases 1 and 2 in full; phase 3 except the job-queue handoff.

- **Client** — `src-tauri/src/transfer.rs` (chunk planning, resume
  subtraction, hashing writer, self-deleting staged/temp files, per-site
  cancel registry; 28 unit tests), `serverkit::push_chunked` /
  `download_resumable`, protocol selection in `sync.rs` via `supports_v2`
  with v1 preserved as one isolated function per operation.
- **Server** — `POST /push/<kind>/init`, `PUT /push/<kind>/chunk`,
  `POST /push/<kind>/finish` in the ServerKit extension, plus `?session=` +
  `conditional=True` on both pulls. v1 and v2 both end in the shared
  `_install_code` / `_import_db`, so there is exactly one processing path.
  `FEATURES` gained `sync-v2`.
- **Memory** — the plan's "tar straight to the pipeline" turned out to matter
  more than the chunking: `snapshot::write_wp_content_tgz` stages the archive
  to a file, `docker::compose_run_reader` streams a dump into
  `wp db import`, and the import untars off disk. Nothing large is buffered
  in either direction anymore.
- **UI** — byte counters on `site-event`, "Pushing wp-content — 148 MB /
  312 MB" in the pinned toast, a Cancel button while bytes move, and a
  `cancelled` terminal stage/history status that reads neutral rather than
  as a failure.
- **Verification** — `m4_smoke` writes a 110 MB incompressible fixture, has
  the mock refuse chunks after two land, and asserts the retry re-sends only
  the missing 14 of 16; the same 123 MB archive is refused over v1 with the
  100 MB error, and v1 still works when `/pair` withholds `sync-v2`.
  `scripts/verify-sync-progress.mjs` covers the UI headlessly.

### Deferred: the server-side job queue

Phase 3's "move `finish`'s processing onto the extension's job queue with a
`GET /jobs/<id>` poll" is **not** implemented. `finish` still processes
inline. The gap it leaves is narrow — a client that disconnects *during
server-side processing* (not during transfer) cannot re-attach to learn the
outcome — and it is partly mitigated: a transfer whose processing fails is
kept rather than discarded, so a retry resumes straight to `finish` instead
of re-uploading. Closing it properly needs job infrastructure the extension
does not have today (ServerKit's `deployment_job_service` is
deployment-specific), which is a larger piece of work than the rest of this
plan combined and belongs in its own slice.

### Notes for whoever picks this up

- Resume needed one thing the plan did not anticipate: `pull/db` and
  `pull/code` *materialize* their payload per request, so plain `Range`
  against them would splice bytes from two different exports. Hence the
  client-generated `?session=` that pins one export server-side. It is a
  small addition to "HTTP already is the chunked protocol here", not a
  replacement for it.
- Adding a third terminal stage (`cancelled`) broke two frontend components
  that hardcoded `done | error` — they stopped clearing `busy` and left the
  push buttons disabled forever. `isTerminalStage` in `stores/sites.ts` is
  now the single list; use it.
