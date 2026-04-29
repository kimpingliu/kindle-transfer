/**
 * Device card used by the device list page.
 */

import { ConnectionBadge } from "./connection-badge";
import { formatDeviceStatus, t } from "../i18n";
import { KindleDeviceView } from "../types";

interface DeviceCardProps {
  device: KindleDeviceView;
  selected: boolean;
  onSelect: (deviceId: string) => void;
}

export function DeviceCard({ device, selected, onSelect }: DeviceCardProps) {
  const usagePercentage = Math.round(
    (device.storageUsedGb / device.storageTotalGb) * 100,
  );
  const statusTone =
    device.status === "ready"
      ? "bg-emerald-300"
      : device.status === "syncing"
        ? "bg-cyan-300"
        : "bg-amber-300";

  return (
    <button
      type="button"
      onClick={() => onSelect(device.id)}
      className={`group panel relative overflow-hidden p-5 text-left transition duration-300 ${selected ? "ring-2 ring-cyan-300/50" : "hover:-translate-y-0.5 hover:ring-1 hover:ring-white/12"}`}
    >
      <div className="absolute inset-x-8 top-0 h-px bg-gradient-to-r from-transparent via-white/25 to-transparent opacity-70" />
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-2">
          <div className="flex items-center gap-3">
            <div className="flex h-11 w-11 items-center justify-center rounded-2xl bg-white/[0.06] ring-1 ring-white/10">
              <DeviceGlyph />
            </div>
            <div>
              <p className="font-display text-base text-slate-50">{device.name}</p>
              <p className="text-sm text-slate-400">{device.model}</p>
            </div>
          </div>
          <div className="flex items-center gap-3">
            <ConnectionBadge connection={device.connection} />
            <span className="inline-flex items-center gap-2 text-xs uppercase tracking-[0.18em] text-slate-500">
              <span className={`h-2 w-2 rounded-full ${statusTone}`} />
              {formatDeviceStatus(device.status)}
            </span>
          </div>
        </div>
        <div className="text-right text-xs text-slate-500">
          <p>{device.lastSeenLabel}</p>
          <p className="mt-1 text-slate-400">FW {device.firmware}</p>
        </div>
      </div>

      <div className="mt-5 grid gap-4 md:grid-cols-3">
        <Metric label={t("device.metric.battery")} value={`${device.batteryLevel}%`} />
        <Metric
          label={t("device.metric.storage")}
          value={`${device.storageUsedGb.toFixed(1)} / ${device.storageTotalGb} GB`}
        />
        <Metric
          label={device.connection === "USB" ? t("device.metric.mount") : t("device.metric.address")}
          value={
            device.connection === "USB"
              ? device.mountPath ?? t("common.unknown")
              : device.ipAddress ?? t("common.unknown")
          }
        />
      </div>

      <div className="mt-5 space-y-3">
        <div className="flex items-center justify-between text-xs">
          <span className="uppercase tracking-[0.16em] text-slate-500">
            {t("device.metric.storageLoad")}
          </span>
          <span className="text-slate-300">{usagePercentage}%</span>
        </div>
        <div className="h-2 overflow-hidden rounded-full bg-white/6">
          <div
            className="h-full rounded-full bg-gradient-to-r from-cyan-400 via-teal-300 to-lime-200"
            style={{ width: `${usagePercentage}%` }}
          />
        </div>
        <div className="flex flex-wrap gap-2">
          {device.supportedFormats.map((format) => (
            <span
              key={format}
              className="rounded-full border border-white/8 bg-white/[0.03] px-2.5 py-1 text-[11px] uppercase tracking-[0.14em] text-slate-300"
            >
              {format}
            </span>
          ))}
        </div>
      </div>
    </button>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-2xl bg-white/[0.03] p-3 ring-1 ring-white/8">
      <p className="text-[11px] uppercase tracking-[0.18em] text-slate-500">{label}</p>
      <p className="mt-2 text-sm text-slate-200">{value}</p>
    </div>
  );
}

function DeviceGlyph() {
  return (
    <svg
      viewBox="0 0 48 48"
      className="h-6 w-6 text-slate-100"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="10" y="6" width="24" height="36" rx="3.5" />
      <path d="M16 14h12" />
      <path d="M17 22c2.4-2.8 5.2-4.2 8.4-4.2 1.4 0 2.6.2 3.6.7" />
      <path d="M18 34c1.5-5.7 4-10 7.5-12.9" />
      <circle cx="31.4" cy="18.6" r="1.4" fill="currentColor" stroke="none" />
    </svg>
  );
}
