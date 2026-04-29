/**
 * Drag-and-drop upload surface.
 *
 * The component only emits files and leaves queue semantics to the page. That
 * keeps it reusable for future “bulk send” and “repair only” flows.
 */

import { useEffect, useId, useRef, useState } from "react";
import { t } from "../i18n";
import { listenToNativeDrop, isTauriRuntime } from "../lib/desktop-bridge";
import { AppRuntimeMode } from "../types";

interface UploadDropzoneProps {
  runtimeMode: AppRuntimeMode;
  isDisabled?: boolean;
  onFilesAdded: (files: File[]) => void;
  onPathsDropped: (paths: string[]) => void;
  onBrowseFiles: () => void;
}

export function UploadDropzone({
  runtimeMode,
  isDisabled = false,
  onFilesAdded,
  onPathsDropped,
  onBrowseFiles,
}: UploadDropzoneProps) {
  const inputId = useId();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const onPathsDroppedRef = useRef(onPathsDropped);
  const disabledRef = useRef(isDisabled);

  useEffect(() => {
    onPathsDroppedRef.current = onPathsDropped;
  }, [onPathsDropped]);

  useEffect(() => {
    disabledRef.current = isDisabled;
  }, [isDisabled]);

  useEffect(() => {
    if (runtimeMode !== "tauri" || !isTauriRuntime()) {
      return;
    }

    let isDisposed = false;
    let teardown: (() => void) | null = null;

    void listenToNativeDrop(
      ({ paths }) => {
        if (!disabledRef.current) {
          onPathsDroppedRef.current(uniquePaths(paths));
        }
      },
      (hovering) => {
        setIsDragging(hovering && !disabledRef.current);
      },
    ).then((unlisten) => {
      if (isDisposed) {
        unlisten();
        return;
      }
      teardown = unlisten;
    });

    return () => {
      isDisposed = true;
      teardown?.();
    };
  }, [runtimeMode]);

  const handleBrowseClick = () => {
    if (isDisabled) {
      return;
    }

    if (runtimeMode === "tauri") {
      onBrowseFiles();
      return;
    }

    inputRef.current?.click();
  };

  return (
    <div
      onDragEnter={() => {
        if (runtimeMode === "browser" && !isDisabled) {
          setIsDragging(true);
        }
      }}
      onDragOver={(event) => {
        if (runtimeMode === "browser" && !isDisabled) {
          event.preventDefault();
          setIsDragging(true);
        }
      }}
      onDragLeave={(event) => {
        if (runtimeMode !== "browser") {
          return;
        }

        event.preventDefault();
        if (event.currentTarget.contains(event.relatedTarget as Node)) {
          return;
        }
        setIsDragging(false);
      }}
      onDrop={(event) => {
        if (runtimeMode !== "browser" || isDisabled) {
          return;
        }

        event.preventDefault();
        setIsDragging(false);
        onFilesAdded(Array.from(event.dataTransfer.files));
      }}
      onClick={handleBrowseClick}
      className={`group relative flex min-h-[250px] flex-col items-center justify-center overflow-hidden rounded-[28px] border border-dashed px-8 py-10 text-center transition duration-300 ${isDisabled ? "cursor-not-allowed border-white/8 bg-white/[0.02] opacity-60" : "cursor-pointer"} ${isDragging ? "border-cyan-300/70 bg-cyan-400/[0.08]" : !isDisabled ? "border-white/12 bg-white/[0.03] hover:border-white/20 hover:bg-white/[0.05]" : ""}`}
    >
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_top,rgba(34,211,238,0.12),transparent_46%),radial-gradient(circle_at_bottom,rgba(132,204,22,0.12),transparent_42%)] opacity-80" />
      <div className="relative space-y-6">
        <div className="mx-auto flex h-16 w-16 items-center justify-center rounded-[22px] bg-white/[0.08] ring-1 ring-white/10 backdrop-blur">
          <UploadGlyph />
        </div>
        <div className="space-y-3">
          <p className="font-display text-2xl text-slate-50">
            {runtimeMode === "tauri"
              ? t("dropzone.tauriTitle")
              : t("dropzone.browserTitle")}
          </p>
          <p className="mx-auto max-w-xl text-sm leading-7 text-slate-400">
            {isDisabled
              ? t("dropzone.disabledDescription")
              : runtimeMode === "tauri"
                ? t("dropzone.tauriDescription")
                : t("dropzone.browserDescription")}
          </p>
        </div>
        <button
          type="button"
          disabled={isDisabled}
          onClick={(event) => {
            event.stopPropagation();
            handleBrowseClick();
          }}
          className="rounded-full border border-cyan-200/20 bg-cyan-300 px-5 py-2.5 text-sm font-medium text-slate-950 transition hover:bg-cyan-200 disabled:cursor-not-allowed disabled:border-white/8 disabled:bg-slate-700 disabled:text-slate-400"
        >
          {t("dropzone.browseButton")}
        </button>
        <div className="flex flex-wrap justify-center gap-2">
          {[
            t("dropzone.feature.bulk"),
            t("dropzone.feature.toc"),
            t("dropzone.feature.format"),
          ].map((feature) => (
            <span
              key={feature}
              className="rounded-full border border-white/10 bg-white/[0.04] px-3 py-1.5 text-[11px] uppercase tracking-[0.18em] text-slate-300"
            >
              {feature}
            </span>
          ))}
        </div>
      </div>
      <input
        ref={inputRef}
        id={inputId}
        type="file"
        accept=".epub,.mobi,.azw3,.pdf"
        multiple
        className="sr-only"
        disabled={runtimeMode === "tauri" || isDisabled}
        onChange={(event) => {
          onFilesAdded(Array.from(event.target.files ?? []));
          event.currentTarget.value = "";
        }}
      />
    </div>
  );
}

function uniquePaths(paths: string[]) {
  const seen = new Set<string>();
  const unique: string[] = [];

  for (const path of paths) {
    const key = path.replace(/\\/g, "/").trim();
    if (key.length === 0 || seen.has(key)) {
      continue;
    }

    seen.add(key);
    unique.push(path);
  }

  return unique;
}

function UploadGlyph() {
  return (
    <svg
      viewBox="0 0 48 48"
      className="h-7 w-7 text-cyan-100"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M24 30V12" />
      <path d="m17 19 7-7 7 7" />
      <path d="M10 31.5v4.2A2.3 2.3 0 0 0 12.3 38h23.4a2.3 2.3 0 0 0 2.3-2.3v-4.2" />
    </svg>
  );
}
