# 12 — Toast notifications

Status: ⬜ not started

A global toast system so actions get visible success/failure feedback
without page-level plumbing. Ported from Faro's `toastStore.ts`, trimmed to
LocalKit's needs.

## Motivation

Today LocalKit has **no success feedback at all**: creating a site, pushing
code/DB, or toggling domains just… finishes (or drops a string into a
page-local error state / the one-off `site-event` progress toast in
`App.tsx`). Errors are `Result<T, String>` that each page surfaces by hand.
Faro solved this with a 91-line Zustand store plus a module-level
`toast.success(...)` helper callable from stores and commands alike — the
single most reused utility in that codebase. It's also the substrate for OS desktop notifications and the
structured-error toasts later (both unplanned Faro-port candidates).

## Design

**`src/stores/toast.ts`** (Zustand, modeled on Faro's
`src/stores/toastStore.ts` — skip the persisted history/unread-count
notification center for v1):

```ts
interface Toast { id: number; kind: "success" | "error" | "info"; title: string; message?: string }
toast.success(title, message?) / toast.error(...) / toast.info(...)  // module-level helpers
```

- Auto-expire: success/info ~4 s, error ~7 s (errors deserve reading time);
  manual dismiss button. Cap the stack at ~4 visible, oldest drops off.
- `toastError(e, context)` helper (Faro `src/lib/errors.ts`): unwraps the
  `string` rejection our commands already return and prefixes the action
  ("Start site", "Push DB").

**Viewport** in `App.tsx` (replaces the two ad-hoc fixed-position toasts
there today — the `progress` site-event toast and the `error` toast get
re-expressed as toasts, keeping the spinner for in-flight stages):

- Fixed bottom-right stack, above modals; dark zinc/violet styling per the
  design system, emerald = success, red = error, border + shadow like the
  existing progress toast. Keep it visually identical to today's toast so
  screenshots stay stable.

**Call sites (v1).** Wire the stores, not every component:

- `stores/sites.ts`: success toasts for create/start/stop/delete;
  `toastError` in the existing `catch` arms instead of only setting `error`.
- `PushPanel` / sync actions: push/pull success + failure (they already
  emit `site-event` stages; toast on `done`/`error`).
- `handleEvent` keeps driving the in-flight progress toast (stage
  files → containers → …), which now is just a pinned info toast that
  resolves into success/error.

## Implementation notes

- Faro references (do not copy blindly — their store has history/unread we
  don't need yet): `Faro/src/stores/toastStore.ts`, `Faro/src/lib/errors.ts`,
  viewport usage in `Faro/src/App.tsx`.
- Mock mode: no backend involvement, toasts are pure frontend — nothing to
  mock.
- Keep the store dependency-free (no icon lib): reuse `components/icons.tsx`
  glyphs (✓ / ✕ / spinner like today).

## Definition of done

- Create/start/stop/delete a site → success toast appears; kill Docker and
  start a site → error toast with the friendly Docker message.
- Push/pull completes → success toast; fails → error toast with context.
- In-flight site creation still shows the staged progress, now rendered by
  the toast system (single implementation, not two).
- Old `progress`/`error` rendering in `App.tsx` removed; stores no longer
  carry a bare `error` string consumed by a one-off toast.
- `npm run build` clean; mock mode shows toasts for all of the above.
