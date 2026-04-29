/**
 * Transfer history list with lightweight status treatments.
 */

import { ConnectionBadge } from "./connection-badge";
import { formatHistoryStatus, t } from "../i18n";
import { HistoryRecord } from "../types";

interface HistoryListProps {
  records: HistoryRecord[];
}

export function HistoryList({ records }: HistoryListProps) {
  return (
    <div className="overflow-hidden rounded-[28px] border border-white/8 bg-white/[0.03]">
      <div className="grid grid-cols-[2.2fr_1.2fr_0.9fr_1fr_0.8fr] gap-4 border-b border-white/8 px-6 py-4 text-[11px] uppercase tracking-[0.2em] text-slate-500">
        <span>{t("history.table.book")}</span>
        <span>{t("history.table.target")}</span>
        <span>{t("history.table.output")}</span>
        <span>{t("history.table.finished")}</span>
        <span>{t("history.table.status")}</span>
      </div>
      <div className="divide-y divide-white/6">
        {records.length === 0 ? (
          <div className="px-6 py-10 text-center text-sm text-slate-400">
            {t("history.empty")}
          </div>
        ) : (
          records.map((record) => (
            <div
              key={record.id}
              className="grid grid-cols-[2.2fr_1.2fr_0.9fr_1fr_0.8fr] gap-4 px-6 py-5 transition hover:bg-white/[0.035]"
            >
              <div className="space-y-1.5">
                <p className="font-medium text-slate-100">{record.title}</p>
                <p className="text-sm text-slate-400">
                  {record.sizeLabel} · {record.durationLabel}
                </p>
              </div>
              <div className="space-y-2">
                <p className="text-sm text-slate-200">{record.deviceName}</p>
                <ConnectionBadge connection={record.connection} />
              </div>
              <div className="flex items-center">
                <span className="rounded-full border border-white/8 bg-white/[0.04] px-3 py-1 text-xs uppercase tracking-[0.16em] text-slate-200">
                  {record.outputFormat}
                </span>
              </div>
              <div className="flex items-center text-sm text-slate-300">
                {record.transferredAt}
              </div>
              <div className="flex items-center">
                <StatusPill status={record.status} />
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function StatusPill({ status }: { status: HistoryRecord["status"] }) {
  const map = {
    success: "border-emerald-400/20 bg-emerald-400/10 text-emerald-200",
    partial: "border-amber-400/20 bg-amber-400/10 text-amber-100",
    failed: "border-rose-400/20 bg-rose-400/10 text-rose-200",
  } as const;

  return (
    <span
      className={`rounded-full border px-3 py-1 text-[11px] uppercase tracking-[0.18em] ${map[status]}`}
    >
      {formatHistoryStatus(status)}
    </span>
  );
}
