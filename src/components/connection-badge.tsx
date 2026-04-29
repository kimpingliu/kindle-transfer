/**
 * Small connection badge used across list and detail surfaces.
 */

import { ConnectionKind } from "../types";

interface ConnectionBadgeProps {
  connection: ConnectionKind;
}

export function ConnectionBadge({ connection }: ConnectionBadgeProps) {
  return (
    <span
      className="inline-flex items-center gap-2 rounded-full border border-emerald-400/20 bg-emerald-400/10 px-2.5 py-1 text-[11px] font-medium uppercase tracking-[0.22em] text-emerald-200"
    >
      <span className="h-1.5 w-1.5 rounded-full bg-emerald-300" />
      {connection}
    </span>
  );
}
