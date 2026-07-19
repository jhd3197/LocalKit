const STYLES: Record<string, string> = {
  running: "bg-emerald-500/15 text-emerald-400 border-emerald-800",
  stopped: "bg-zinc-500/15 text-zinc-400 border-zinc-700",
  creating: "bg-amber-500/15 text-amber-400 border-amber-800",
  error: "bg-red-500/15 text-red-400 border-red-800",
};

export default function StatusBadge({ status }: { status: string }) {
  const style = STYLES[status] ?? STYLES.stopped;
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium capitalize ${style}`}
    >
      <span
        className={`h-1.5 w-1.5 rounded-full bg-current ${status === "creating" ? "animate-pulse" : ""}`}
      />
      {status}
    </span>
  );
}
