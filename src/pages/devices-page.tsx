/**
 * Device list page.
 */

import { DeviceCard } from "../components/device-card";
import { t } from "../i18n";
import { KindleDeviceView } from "../types";

interface DevicesPageProps {
  devices: KindleDeviceView[];
  selectedDeviceId: string;
  onSelectDevice: (deviceId: string) => void;
}

export function DevicesPage({
  devices,
  selectedDeviceId,
  onSelectDevice,
}: DevicesPageProps) {
  const onlineCount = devices.length;
  const usbCount = devices.filter((device) => device.connection === "USB").length;

  return (
    <div className="space-y-8">
      <header>
        <section className="grid gap-4 sm:grid-cols-2">
          <SummaryCard
            label={t("devices.summary.online")}
            value={String(onlineCount)}
            detail={t("devices.summary.onlineDetail")}
          />
          <SummaryCard label="USB" value={String(usbCount)} detail={t("devices.summary.usbDetail")} />
        </section>
      </header>

      <section className="grid gap-5 xl:grid-cols-2">
        {devices.length === 0 ? (
          <div className="panel col-span-full p-7 text-sm leading-7 text-slate-400">
            <p className="font-display text-2xl text-slate-50">{t("devices.emptyTitle")}</p>
            <p className="mt-3">{t("devices.emptyBody")}</p>
          </div>
        ) : (
          devices.map((device) => (
            <DeviceCard
              key={device.id}
              device={device}
              selected={selectedDeviceId === device.id}
              onSelect={onSelectDevice}
            />
          ))
        )}
      </section>
    </div>
  );
}

function SummaryCard({
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
      <p className="text-[11px] uppercase tracking-[0.2em] text-slate-500">{label}</p>
      <p className="mt-3 font-display text-3xl text-slate-50">{value}</p>
      <p className="mt-2 text-sm text-slate-400">{detail}</p>
    </div>
  );
}
