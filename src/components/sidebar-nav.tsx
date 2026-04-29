/**
 * Sidebar navigation for the desktop shell.
 */

import { BrandMark } from "./brand-mark";
import { t } from "../i18n";

type ViewKey = "devices" | "upload" | "library" | "history";

interface SidebarNavProps {
  activeView: ViewKey;
  onChange: (view: ViewKey) => void;
}

export function SidebarNav({ activeView, onChange }: SidebarNavProps) {
  const navigation = [
    {
      id: "devices" as const,
      label: t("view.devices"),
      note: t("sidebar.devicesNote"),
      icon: DevicesIcon,
    },
    {
      id: "upload" as const,
      label: t("view.upload"),
      note: t("sidebar.uploadNote"),
      icon: UploadIcon,
    },
    {
      id: "library" as const,
      label: t("view.library"),
      note: t("sidebar.libraryNote"),
      icon: LibraryIcon,
    },
    {
      id: "history" as const,
      label: t("view.history"),
      note: t("sidebar.historyNote"),
      icon: HistoryIcon,
    },
  ];

  return (
    <aside className="relative flex h-full flex-col rounded-[32px] border border-white/8 bg-[#0b0f16]/90 p-5 shadow-[0_24px_80px_rgba(0,0,0,0.45)] backdrop-blur-xl">
      <div className="space-y-8">
        <div className="space-y-3 px-2">
          <div className="flex items-center gap-3">
            <BrandMark className="h-12 w-12 shrink-0" />
            <div>
              <p className="font-display text-lg text-slate-50">{t("app.name")}</p>
            </div>
          </div>
        </div>

        <nav className="space-y-2">
          {navigation.map((item) => {
            const Icon = item.icon;
            const active = activeView === item.id;

            return (
              <button
                key={item.id}
                type="button"
                onClick={() => onChange(item.id)}
                className={`flex w-full items-center gap-4 rounded-[20px] px-4 py-3 text-left transition ${active ? "bg-white/[0.08] text-slate-50 ring-1 ring-white/10" : "text-slate-400 hover:bg-white/[0.04] hover:text-slate-200"}`}
              >
                <span
                  className={`flex h-10 w-10 items-center justify-center rounded-2xl ${active ? "bg-cyan-300/12 text-cyan-100" : "bg-white/[0.03] text-slate-400"}`}
                >
                  <Icon />
                </span>
                <span className="min-w-0">
                  <span className="block font-medium">{item.label}</span>
                  <span className="block truncate text-xs uppercase tracking-[0.18em] text-slate-500">
                    {item.note}
                  </span>
                </span>
              </button>
            );
          })}
        </nav>
      </div>
    </aside>
  );
}

function DevicesIcon() {
  return (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.8">
      <rect x="4" y="3" width="8" height="18" rx="2.2" />
      <rect x="14" y="7" width="6" height="10" rx="1.8" />
    </svg>
  );
}

function UploadIcon() {
  return (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M12 16V4" />
      <path d="m7 9 5-5 5 5" />
      <path d="M4 18.5V20h16v-1.5" />
    </svg>
  );
}

function LibraryIcon() {
  return (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M5 5.5A2.5 2.5 0 0 1 7.5 3H19v16H7.5A2.5 2.5 0 0 0 5 21V5.5Z" />
      <path d="M5 5.5A2.5 2.5 0 0 0 7.5 8H19" />
      <path d="M9 12h6" />
      <path d="M9 15h4" />
    </svg>
  );
}

function HistoryIcon() {
  return (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M20 12a8 8 0 1 1-2.34-5.66" />
      <path d="M20 4v6h-6" />
      <path d="M12 8v5l3 2" />
    </svg>
  );
}
