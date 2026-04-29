/**
 * Transfer history page.
 */

import { startTransition, useDeferredValue, useState } from "react";
import { HistoryList } from "../components/history-list";
import { t } from "../i18n";
import { HistoryRecord } from "../types";

interface HistoryPageProps {
  records: HistoryRecord[];
}

export function HistoryPage({ records }: HistoryPageProps) {
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | HistoryRecord["status"]>("all");
  const deferredQuery = useDeferredValue(query);

  const filteredRecords = records.filter((record) => {
    const normalizedQuery = deferredQuery.trim().toLowerCase();
    const matchesQuery =
      normalizedQuery.length === 0 ||
      record.title.toLowerCase().includes(normalizedQuery) ||
      record.deviceName.toLowerCase().includes(normalizedQuery) ||
      record.outputFormat.toLowerCase().includes(normalizedQuery);
    const matchesStatus =
      statusFilter === "all" || record.status === statusFilter;

    return matchesQuery && matchesStatus;
  });

  return (
    <div className="space-y-8">
      <header className="grid gap-6 xl:grid-cols-[1.2fr_0.95fr]">
        <section className="panel p-7">
          <p className="text-xs uppercase tracking-[0.22em] text-slate-500">
            {t("history.eyebrow")}
          </p>
          <h1 className="mt-3 font-display text-4xl leading-tight text-slate-50">
            {t("history.title")}
          </h1>
          <p className="mt-4 max-w-2xl text-sm leading-7 text-slate-400">
            {t("history.description")}
          </p>
        </section>

        <section className="grid gap-4 sm:grid-cols-3 xl:grid-cols-3">
          <HistoryMetric
            label={t("history.metric.total")}
            value={String(records.length)}
            detail={t("history.metric.totalDetail")}
          />
          <HistoryMetric
            label={t("history.metric.success")}
            value={String(records.filter((record) => record.status === "success").length)}
            detail={t("history.metric.successDetail")}
          />
          <HistoryMetric
            label={t("history.metric.review")}
            value={String(records.filter((record) => record.status !== "success").length)}
            detail={t("history.metric.reviewDetail")}
          />
        </section>
      </header>

      <section className="panel p-6">
        <div className="grid gap-4 lg:grid-cols-[1.4fr_0.7fr]">
          <label className="flex items-center gap-3 rounded-[20px] border border-white/8 bg-white/[0.03] px-4 py-3">
            <SearchGlyph />
            <input
              value={query}
              onChange={(event) => {
                const nextValue = event.target.value;
                startTransition(() => {
                  setQuery(nextValue);
                });
              }}
              placeholder={t("history.searchPlaceholder")}
              className="w-full bg-transparent text-sm text-slate-100 placeholder:text-slate-500 focus:outline-none"
            />
          </label>

          <FilterSelect
            value={statusFilter}
            onChange={(value) =>
              setStatusFilter(value as "all" | HistoryRecord["status"])
            }
            options={[
              ["all", t("history.filter.allStatus")],
              ["success", t("history.filter.success")],
              ["partial", t("history.filter.partial")],
              ["failed", t("history.filter.failed")],
            ]}
          />
        </div>
      </section>

      <HistoryList records={filteredRecords} />
    </div>
  );
}

function HistoryMetric({
  label,
  value,
  detail,
}: {
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <div className="panel p-5">
      <p className="text-[11px] uppercase tracking-[0.18em] text-slate-500">{label}</p>
      <p className="mt-3 font-display text-3xl text-slate-50">{value}</p>
      <p className="mt-2 text-sm text-slate-400">{detail}</p>
    </div>
  );
}

function FilterSelect({
  value,
  onChange,
  options,
}: {
  value: string;
  onChange: (value: string) => void;
  options: Array<[string, string]>;
}) {
  return (
    <select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className="rounded-[20px] border border-white/8 bg-white/[0.03] px-4 py-3 text-sm text-slate-200 outline-none transition hover:border-white/14 focus:border-cyan-300/35"
    >
      {options.map(([optionValue, optionLabel]) => (
        <option key={optionValue} value={optionValue} className="bg-[#10151d]">
          {optionLabel}
        </option>
      ))}
    </select>
  );
}

function SearchGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="h-5 w-5 text-slate-500"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </svg>
  );
}
