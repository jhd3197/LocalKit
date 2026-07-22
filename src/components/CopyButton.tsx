import { useState } from "react";
import { CheckIcon, DuplicateIcon } from "./icons";

export default function CopyButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // clipboard unavailable; ignore
    }
  };

  return (
    <button
      onClick={copy}
      className={`inline-flex items-center gap-1 rounded border px-2 py-0.5 text-xs transition-colors ${
        copied
          ? "border-emerald-800 text-emerald-400"
          : "border-zinc-700 text-zinc-400 hover:border-zinc-500 hover:text-zinc-200"
      }`}
    >
      {copied ? <CheckIcon className="h-3 w-3" /> : <DuplicateIcon className="h-3 w-3" />}
      {copied ? "Copied!" : "Copy"}
    </button>
  );
}
