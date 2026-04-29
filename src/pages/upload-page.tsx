/**
 * Book upload page.
 *
 * The page simulates a conversion and transfer pipeline so the desktop UI feels
 * alive before the Rust backend is connected. The state shape mirrors the
 * expected backend job model.
 */

import { ConnectionBadge } from "../components/connection-badge";
import { ProgressBar } from "../components/progress-bar";
import { UploadDropzone } from "../components/upload-dropzone";
import { formatMessage, formatQueueStageCaption, t } from "../i18n";
import { AppRuntimeMode, KindleDeviceView, UploadQueueItem } from "../types";

interface UploadPageProps {
  devices: KindleDeviceView[];
  queue: UploadQueueItem[];
  runtimeMode: AppRuntimeMode;
  isProcessing: boolean;
  selectedDeviceId: string;
  onSelectDevice: (deviceId: string) => void;
  onFilesAdded: (files: File[]) => void;
  onPathsDropped: (paths: string[]) => void;
  onBrowseFiles: () => void;
  onStartTransfer: () => void;
}

export function UploadPage({
  devices,
  queue,
  runtimeMode,
  isProcessing,
  selectedDeviceId,
  onSelectDevice,
  onFilesAdded,
  onPathsDropped,
  onBrowseFiles,
  onStartTransfer,
}: UploadPageProps) {
  const selectedDevice =
    devices.find((device) => device.id === selectedDeviceId) ?? devices[0] ?? null;

  const totalFiles = queue.length;
  const finishedFiles = queue.filter((item) => item.stage === "done").length;
  const aggregateProgress =
    totalFiles === 0
      ? 0
      : Math.round(
          queue.reduce((sum, item) => sum + (item.convertProgress + item.uploadProgress) / 2, 0) /
            totalFiles,
        );

  return (
    <div className="space-y-8">
      <header>
        <section className="panel p-6">
          <p className="text-[11px] uppercase tracking-[0.2em] text-slate-500">
            {t("upload.currentTarget")}
          </p>
          {selectedDevice ? (
            <div className="mt-4 space-y-4">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="font-display text-xl text-slate-50">{selectedDevice.name}</p>
                  <p className="mt-1 text-sm text-slate-400">{selectedDevice.model}</p>
                </div>
                <ConnectionBadge connection={selectedDevice.connection} />
              </div>
              <div className="grid gap-3 sm:grid-cols-2">
                {devices.map((device) => (
                  <button
                    key={device.id}
                    type="button"
                    onClick={() => onSelectDevice(device.id)}
                    className={`rounded-2xl border px-4 py-3 text-left transition ${selectedDevice.id === device.id ? "border-cyan-300/35 bg-cyan-400/[0.08] text-slate-50" : "border-white/8 bg-white/[0.03] text-slate-400 hover:border-white/14 hover:text-slate-200"}`}
                  >
                    <p className="text-sm font-medium">{device.name}</p>
                    <p className="mt-1 text-xs uppercase tracking-[0.18em]">
                      {device.connection}
                    </p>
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <p className="mt-4 text-sm text-slate-400">{t("upload.noDevice")}</p>
          )}
        </section>
      </header>

      <div className="grid gap-6 xl:grid-cols-[1.08fr_0.92fr]">
        <section className="space-y-6">
          <UploadDropzone
            runtimeMode={runtimeMode}
            isDisabled={!selectedDevice}
            onFilesAdded={onFilesAdded}
            onPathsDropped={onPathsDropped}
            onBrowseFiles={onBrowseFiles}
          />

          <div className="panel p-6">
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div>
                <p className="text-[11px] uppercase tracking-[0.2em] text-slate-500">
                  {t("upload.batchState")}
                </p>
                <p className="mt-2 font-display text-2xl text-slate-50">
                  {formatMessage("upload.finishedCount", {
                    done: finishedFiles,
                    total: totalFiles,
                  })}
                </p>
              </div>
              <button
                type="button"
                disabled={
                  !selectedDevice
                  || (runtimeMode === "tauri" && isProcessing)
                }
                onClick={onStartTransfer}
                className="rounded-full bg-cyan-300 px-5 py-3 text-sm font-medium text-slate-950 transition hover:bg-cyan-200 disabled:cursor-not-allowed disabled:bg-slate-600 disabled:text-slate-300"
              >
                {runtimeMode === "tauri"
                  ? isProcessing
                    ? t("upload.transferRunning")
                    : t("upload.startTransfer")
                  : isProcessing
                    ? t("upload.pausePipeline")
                    : t("upload.startTransfer")}
              </button>
            </div>

            <div className="mt-6 space-y-4">
              <ProgressBar
                label={t("upload.overallProgress")}
                value={aggregateProgress}
                accent="cyan"
              />
              <div className="grid gap-4 sm:grid-cols-2">
                <MiniStat
                  label={t("upload.targetFormatBias")}
                  value="AZW3"
                />
                <MiniStat
                  label={t("upload.queueWeight")}
                  value={`${queue.reduce((sum, item) => sum + item.sizeMb, 0).toFixed(1)} MB`}
                />
              </div>
            </div>
          </div>
        </section>

        <section className="panel p-6">
          <div className="flex items-center justify-between gap-4">
            <div>
              <p className="text-[11px] uppercase tracking-[0.2em] text-slate-500">
                {t("upload.activeQueue")}
              </p>
              <p className="mt-2 font-display text-2xl text-slate-50">
                {queue.length === 0
                  ? t("upload.queueClear")
                  : formatMessage("upload.processingCount", { count: queue.length })}
              </p>
            </div>
            <span className="rounded-full border border-white/8 bg-white/[0.04] px-3 py-1 text-xs uppercase tracking-[0.18em] text-slate-300">
              {t("upload.live")}
            </span>
          </div>

          <div className="mt-6 space-y-4">
            {queue.length === 0 ? (
              <div className="rounded-[24px] border border-white/8 bg-white/[0.03] p-6 text-sm text-slate-400">
                {t("upload.dropToStart")}
              </div>
            ) : (
              queue.map((item) => (
                <article
                  key={item.id}
                  className="rounded-[24px] border border-white/8 bg-white/[0.03] p-5 transition hover:bg-white/[0.045]"
                >
                  <div className="flex items-start justify-between gap-4">
                    <div>
                      <p className="font-medium text-slate-100">{item.title}</p>
                      <p className="mt-1 text-sm text-slate-400">
                        {item.author} · {item.sizeMb} MB
                      </p>
                    </div>
                    <span className="rounded-full border border-white/8 bg-white/[0.04] px-3 py-1 text-[11px] uppercase tracking-[0.18em] text-slate-300">
                      {item.sourceFormat} → {item.targetFormat}
                    </span>
                  </div>

                  <div className="mt-5 space-y-4">
                    <ProgressBar
                      label={t("upload.progress.conversion")}
                      value={item.convertProgress}
                      accent="amber"
                      caption={formatQueueStageCaption(item.stage, "convert")}
                    />
                    <ProgressBar
                      label={t("upload.progress.upload")}
                      value={item.uploadProgress}
                      accent="emerald"
                      caption={formatQueueStageCaption(item.stage, "upload")}
                    />
                    {item.stage === "done" && item.destinationPath ? (
                      <div className="rounded-[18px] border border-emerald-300/12 bg-emerald-300/[0.06] px-4 py-3">
                        <p className="text-[11px] uppercase tracking-[0.18em] text-emerald-100/75">
                          {t("upload.writtenTo")}
                        </p>
                        <p className="mt-2 break-all text-sm leading-6 text-emerald-50/92">
                          {item.destinationPath}
                        </p>
                      </div>
                    ) : null}
                  </div>
                </article>
              ))
            )}
          </div>
        </section>
      </div>
    </div>
  );
}

function MiniStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[20px] border border-white/8 bg-white/[0.03] p-4">
      <p className="text-[11px] uppercase tracking-[0.18em] text-slate-500">{label}</p>
      <p className="mt-2 text-sm text-slate-200">{value}</p>
    </div>
  );
}
