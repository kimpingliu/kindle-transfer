/**
 * Main desktop shell.
 *
 * The application uses a lightweight tab model instead of a router so the Tauri
 * desktop window stays fast and easy to reason about while the product is still
 * backend-heavy.
 */

import { startTransition, useEffect, useState } from "react";
import { SidebarNav } from "./components/sidebar-nav";
import { useDesktopState } from "./hooks/use-desktop-state";
import {
  getCurrentLocale,
  LOCALE_OPTIONS,
  setCurrentLocale,
  t,
  type Locale,
} from "./i18n";
import { DevicesPage } from "./pages/devices-page";
import { HistoryPage } from "./pages/history-page";
import { LibraryPage } from "./pages/library-page";
import { UploadPage } from "./pages/upload-page";

type ViewKey = "devices" | "upload" | "library" | "history";

export default function App() {
  const [activeView, setActiveView] = useState<ViewKey>("devices");
  const [locale, setLocale] = useState<Locale>(() => getCurrentLocale());
  const {
    runtimeMode,
    snapshot,
    isProcessing,
    browseNativeFiles,
    deleteDeviceBook,
    queueBrowserFiles,
    queueNativePaths,
    loadDeviceBooks,
    renameDeviceBook,
    startTransfer,
  } = useDesktopState();
  const [selectedDeviceId, setSelectedDeviceId] = useState(snapshot.devices[0]?.id ?? "");

  useEffect(() => {
    if (snapshot.devices.length === 0) {
      if (selectedDeviceId !== "") {
        setSelectedDeviceId("");
      }
      return;
    }

    if (!snapshot.devices.some((device) => device.id === selectedDeviceId)) {
      setSelectedDeviceId(snapshot.devices[0]?.id ?? "");
    }
  }, [selectedDeviceId, snapshot.devices]);

  function handleLocaleChange(nextLocale: Locale) {
    startTransition(() => {
      setCurrentLocale(nextLocale);
      setLocale(nextLocale);
    });
  }

  return (
    <div className="min-h-screen bg-[#070b11] text-slate-100 antialiased">
      <div className="fixed inset-0 bg-[radial-gradient(circle_at_top_left,rgba(34,211,238,0.12),transparent_28%),radial-gradient(circle_at_top_right,rgba(196,181,253,0.05),transparent_24%),radial-gradient(circle_at_bottom_left,rgba(132,204,22,0.09),transparent_34%)]" />
      <div className="fixed inset-0 bg-[linear-gradient(180deg,rgba(255,255,255,0.02),transparent_22%,transparent_78%,rgba(255,255,255,0.02))]" />

      <div className="relative mx-auto flex min-h-screen max-w-[1680px] gap-5 p-4 lg:p-6">
        <div className="hidden w-[300px] shrink-0 lg:block">
          <SidebarNav
            activeView={activeView}
            onChange={(view) => {
              startTransition(() => {
                setActiveView(view);
              });
            }}
          />
        </div>

        <main className="flex-1 overflow-hidden rounded-[32px] border border-white/8 bg-[#0c1118]/88 p-5 shadow-[0_28px_90px_rgba(0,0,0,0.35)] backdrop-blur-xl lg:p-7">
          <TopBar
            activeView={activeView}
            locale={locale}
            onChange={(view) => {
              startTransition(() => {
                setActiveView(view);
              });
            }}
            onLocaleChange={handleLocaleChange}
          />

          <div className="mt-8 animate-fade-up">
            {activeView === "devices" ? (
              <DevicesPage
                devices={snapshot.devices}
                selectedDeviceId={selectedDeviceId}
                onSelectDevice={(deviceId) => {
                  startTransition(() => {
                    setSelectedDeviceId(deviceId);
                  });
                }}
              />
            ) : activeView === "upload" ? (
              <UploadPage
                devices={snapshot.devices}
                queue={snapshot.uploadQueue}
                runtimeMode={runtimeMode}
                isProcessing={isProcessing}
                selectedDeviceId={selectedDeviceId}
                onSelectDevice={(deviceId) => {
                  startTransition(() => {
                    setSelectedDeviceId(deviceId);
                  });
                }}
                onFilesAdded={(files) => {
                  queueBrowserFiles(selectedDeviceId, files);
                }}
                onPathsDropped={(paths) => {
                  void queueNativePaths(selectedDeviceId, paths);
                }}
                onBrowseFiles={() => {
                  void browseNativeFiles(selectedDeviceId);
                }}
                onStartTransfer={() => {
                  void startTransfer(selectedDeviceId);
                }}
              />
            ) : activeView === "library" ? (
              <LibraryPage
                devices={snapshot.devices}
                runtimeMode={runtimeMode}
                selectedDeviceId={selectedDeviceId}
                onSelectDevice={(deviceId) => {
                  startTransition(() => {
                    setSelectedDeviceId(deviceId);
                  });
                }}
                onLoadBooks={loadDeviceBooks}
                onDeleteBook={deleteDeviceBook}
                onRenameBook={renameDeviceBook}
              />
            ) : (
              <HistoryPage records={snapshot.history} />
            )}
          </div>
        </main>
      </div>
    </div>
  );
}

function TopBar({
  activeView,
  locale,
  onChange,
  onLocaleChange,
}: {
  activeView: ViewKey;
  locale: Locale;
  onChange: (view: ViewKey) => void;
  onLocaleChange: (locale: Locale) => void;
}) {
  return (
    <div className="flex flex-col gap-5 border-b border-white/8 pb-5 lg:flex-row lg:items-center lg:justify-between">
      <div className="space-y-1">
        <p className="text-[11px] uppercase tracking-[0.24em] text-slate-500">
          {t("app.topbarEyebrow")}
        </p>
        <p className="font-display text-2xl text-slate-50">
          {activeView === "devices"
            ? t("view.devicesTitle")
            : activeView === "upload"
              ? t("view.uploadTitle")
              : activeView === "library"
                ? t("view.libraryTitle")
                : t("view.historyTitle")}
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-3 self-start lg:justify-end">
        <label className="flex items-center gap-2 rounded-full border border-white/8 bg-white/[0.03] px-3 py-1.5 text-xs uppercase tracking-[0.18em] text-slate-400">
          <span>Language</span>
          <select
            value={locale}
            onChange={(event) => onLocaleChange(event.target.value as Locale)}
            className="cursor-pointer appearance-none rounded-full border border-white/8 bg-[#0f1722] px-3 py-1.5 text-sm normal-case tracking-normal text-slate-100 outline-none transition hover:border-cyan-300/40 focus:border-cyan-300/55"
          >
            {LOCALE_OPTIONS.map((option) => (
              <option
                key={option.value}
                value={option.value}
                className="bg-[#0f1722] text-slate-100"
              >
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <div className="flex items-center gap-3 rounded-full border border-white/8 bg-white/[0.03] p-1">
          {([
            ["devices", t("view.devices")],
            ["upload", t("view.upload")],
            ["library", t("view.library")],
            ["history", t("view.history")],
          ] as Array<[ViewKey, string]>).map(([view, label]) => (
            <button
              key={view}
              type="button"
              onClick={() => onChange(view)}
              className={`rounded-full px-4 py-2 text-sm transition ${activeView === view ? "bg-cyan-300 text-slate-950" : "text-slate-400 hover:text-slate-200"}`}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
