# CLAUDE.md — LocalKit

Local WordPress/PHP/Docker development sites, managed as per-site Docker
Compose projects — Tauri 2 desktop app (React + Rust) plus the `lk` CLI.
Push/pull syncs with a ServerKit server via the `serverkit-localkit`
extension.

**Read [`AGENTS.md`](AGENTS.md) first.** It is the single source of truth for
this repo: project structure, build/test commands, and the binding
conventions (Docker CLI only, forward-only migrations, capability-gated site
kinds, settings store, events, sync protocol). Do not duplicate it here —
when conventions change, update AGENTS.md.

## Git & release workflow — read this first

**Two branches, nothing else: `main` and `dev`.**

- **Do all work on `dev`.** Never create per-feature branches (`feat/…`,
  `fix/…`, `chore/…`) — stay on `dev`.
- **Commit locally; never push.** Make small, focused commits on `dev` as you
  go. **Do not `git push`, do not merge into `main`, do not open PRs.** The
  maintainer reviews the local commits and pushes / merges to `main` himself.
- Merging `dev` into `main` is what triggers the release workflow
  (`.github/workflows/release.yml` auto-bumps, tags, and publishes) — another
  reason merges are never an agent's job. Use `[skip ci]` in a commit message
  when a main push should not release.

## Working agreements

- **Plans:** numbered implementation plans live in `docs/plans/` with
  `ROADMAP.md` as the tracker. New feature work starts as a plan file (next
  number, `Status: ⬜ planned`) and the plan header + ROADMAP row are marked
  shipped in the same commit series that finishes it.
- **Verification before "done":** `npm run build`, `npm run test` (vitest),
  and `cargo check --workspace --all-targets` must pass; features with a
  headless check add one under `scripts/verify-*.mjs`, and Docker-backed
  flows extend the `smoke` / `docker_smoke` / `m4_smoke` examples. See the
  full list in AGENTS.md → Build / test commands.
- **Both frontends stay thin:** logic lives in `localkit_lib`; the Tauri
  commands and the `lk` CLI are wrappers. If the CLI can't do something the
  GUI can, that's a smell.
- **PR descriptions** (when the maintainer asks for one) follow the
  `create-pr` skill in `.claude/skills/`; generated files land in `.pr/`
  (gitignored).
