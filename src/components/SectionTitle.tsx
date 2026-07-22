import type { ComponentType } from "react";

/**
 * Panel heading with a leading icon (plan 27). Renders the same `h2` the
 * screenshot script and tests match by text — icons are aria-hidden SVGs, so
 * `textContent` is unchanged.
 */
export default function SectionTitle({
  icon: Icon,
  children,
}: {
  icon: ComponentType<{ className?: string }>;
  children: React.ReactNode;
}) {
  return (
    <h2 className="flex items-center gap-2 text-sm font-semibold uppercase tracking-wide text-zinc-500">
      <Icon className="h-3.5 w-3.5 shrink-0 text-zinc-600" />
      {children}
    </h2>
  );
}
