/**
 * Shared desktop state hook.
 *
 * The hook handles two runtime modes:
 *
 * - `tauri`: the UI is backed by Rust commands and emitted state updates.
 * - `browser`: mock data and simulated queue progress remain available for UI work.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { mockDevices, mockHistory, mockUploadQueue } from "../data/mock";
import {
  getAppState,
  isTauriRuntime,
  deleteKindleBook,
  listKindleBooks,
  listenToStateChanges,
  openUploadFilePicker,
  queueUploadFiles,
  refreshDevices,
  renameKindleBook,
  startUpload,
} from "../lib/desktop-bridge";
import { t } from "../i18n";
import type {
  AppRuntimeMode,
  FrontendStateSnapshot,
  UploadQueueItem,
} from "../types";

const MOCK_STATE: FrontendStateSnapshot = {
  devices: mockDevices,
  history: mockHistory,
  uploadQueue: mockUploadQueue,
};

export function useDesktopState() {
  const [runtimeMode, setRuntimeMode] = useState<AppRuntimeMode>(
    isTauriRuntime() ? "tauri" : "browser",
  );
  const [snapshot, setSnapshot] = useState<FrontendStateSnapshot>(MOCK_STATE);
  const [isMockProcessing, setIsMockProcessing] = useState(false);
  const hasActiveUploadRef = useRef(false);

  useEffect(() => {
    if (runtimeMode !== "tauri") {
      return;
    }

    let isDisposed = false;
    let teardown: (() => void) | null = null;

    void (async () => {
      try {
        const [initialState, stopListening] = await Promise.all([
          getAppState(),
          listenToStateChanges((nextState) => {
            if (!isDisposed) {
              setSnapshot(nextState);
            }
          }),
        ]);

        if (isDisposed) {
          stopListening();
          return;
        }

        teardown = stopListening;
        setSnapshot(initialState);
        setSnapshot(await refreshDevices());
      } catch (error) {
        console.error("Falling back to browser mode:", error);
        setRuntimeMode("browser");
        setSnapshot(MOCK_STATE);
      }
    })();

    const interval = window.setInterval(() => {
      if (hasActiveUploadRef.current) {
        return;
      }

      void refreshDevices()
        .then((nextState) => {
          if (!isDisposed) {
            setSnapshot(nextState);
          }
        })
        .catch((error) => {
          console.warn("Background device refresh failed:", error);
        });
    }, 15000);

    return () => {
      isDisposed = true;
      window.clearInterval(interval);
      teardown?.();
    };
  }, [runtimeMode]);

  useEffect(() => {
    if (runtimeMode !== "browser" || !isMockProcessing) {
      return;
    }

    const timer = window.setInterval(() => {
      let shouldStop = false;

      setSnapshot((currentSnapshot) => {
        const nextQueue = currentSnapshot.uploadQueue.map((item) => ({ ...item }));
        const currentItem = nextQueue.find(
          (item) => item.stage !== "done" && item.stage !== "failed",
        );

        if (!currentItem) {
          shouldStop = true;
          return currentSnapshot;
        }

        if (currentItem.stage === "queued") {
          currentItem.stage = "converting";
          currentItem.convertProgress = 8;
          return {
            ...currentSnapshot,
            uploadQueue: nextQueue,
          };
        }

        if (currentItem.stage === "converting") {
          currentItem.convertProgress = Math.min(
            100,
            currentItem.convertProgress + 12,
          );

          if (currentItem.convertProgress >= 100) {
            currentItem.stage = "uploading";
          }

          return {
            ...currentSnapshot,
            uploadQueue: nextQueue,
          };
        }

        if (currentItem.stage === "uploading") {
          currentItem.uploadProgress = Math.min(100, currentItem.uploadProgress + 15);
          if (currentItem.uploadProgress >= 100) {
            currentItem.stage = "verifying";
          }

          return {
            ...currentSnapshot,
            uploadQueue: nextQueue,
          };
        }

        if (currentItem.stage === "verifying") {
          currentItem.stage = "done";
          return {
            ...currentSnapshot,
            uploadQueue: nextQueue,
          };
        }

        return currentSnapshot;
      });

      if (shouldStop) {
        setIsMockProcessing(false);
      }
    }, 640);

    return () => {
      window.clearInterval(timer);
    };
  }, [runtimeMode, isMockProcessing]);

  const isProcessing = useMemo(() => {
    if (runtimeMode === "browser") {
      return isMockProcessing;
    }

    return snapshot.uploadQueue.some((item) =>
      ["converting", "uploading", "verifying"].includes(item.stage),
    );
  }, [runtimeMode, isMockProcessing, snapshot.uploadQueue]);

  useEffect(() => {
    hasActiveUploadRef.current = isProcessing;
  }, [isProcessing]);

  return {
    runtimeMode,
    snapshot,
    isProcessing,
    async queueNativePaths(deviceId: string, filePaths: string[]) {
      if (runtimeMode !== "tauri") {
        return;
      }

      const uniquePaths = dedupeNativePaths(filePaths);
      if (deviceId.trim().length === 0 || uniquePaths.length === 0) {
        return;
      }

      setSnapshot(await queueUploadFiles(deviceId, uniquePaths));
    },
    async browseNativeFiles(deviceId: string) {
      if (runtimeMode !== "tauri" || deviceId.trim().length === 0) {
        return;
      }

      const selectedPaths = dedupeNativePaths(await openUploadFilePicker());
      if (selectedPaths.length === 0) {
        return;
      }

      setSnapshot(await queueUploadFiles(deviceId, selectedPaths));
    },
    queueBrowserFiles(deviceId: string, files: File[]) {
      if (runtimeMode !== "browser" || deviceId.trim().length === 0 || files.length === 0) {
        return;
      }

      setSnapshot((currentSnapshot) => {
        const nextQueue = [...currentSnapshot.uploadQueue];

        for (const file of dedupeBrowserFiles(files)) {
          const nextItem: UploadQueueItem = {
            id: browserQueueItemId(deviceId, file),
            title: file.name.replace(/\.[^.]+$/, ""),
            author: t("upload.localImport"),
            sourceFormat: file.name.split(".").pop()?.toUpperCase() ?? "FILE",
            targetFormat: "AZW3",
            sizeMb: Number((file.size / 1024 / 1024).toFixed(1)),
            convertProgress: 0,
            uploadProgress: 0,
            stage: "queued",
            destinationPath: undefined,
          };

          const existingIndex = nextQueue.findIndex((item) => item.id === nextItem.id);
          if (existingIndex >= 0) {
            nextQueue[existingIndex] = nextItem;
          } else {
            nextQueue.push(nextItem);
          }
        }

        return {
          ...currentSnapshot,
          uploadQueue: nextQueue,
        };
      });
    },
    async startTransfer(deviceId: string) {
      if (deviceId.trim().length === 0) {
        return;
      }

      if (runtimeMode === "tauri") {
        setSnapshot(await startUpload(deviceId));
        return;
      }

      setIsMockProcessing((currentValue) => !currentValue);
    },
    async loadDeviceBooks(deviceId: string) {
      if (runtimeMode !== "tauri" || deviceId.trim().length === 0) {
        return [];
      }

      return listKindleBooks(deviceId);
    },
    async deleteDeviceBook(deviceId: string, bookId: string) {
      if (runtimeMode !== "tauri" || deviceId.trim().length === 0 || bookId.trim().length === 0) {
        return;
      }

      await deleteKindleBook(deviceId, bookId);
    },
    async renameDeviceBook(deviceId: string, bookId: string, title: string) {
      if (
        runtimeMode !== "tauri" ||
        deviceId.trim().length === 0 ||
        bookId.trim().length === 0 ||
        title.trim().length === 0
      ) {
        return null;
      }

      return renameKindleBook(deviceId, bookId, title);
    },
  };
}

function dedupeNativePaths(filePaths: string[]) {
  const seen = new Set<string>();
  const unique: string[] = [];

  for (const filePath of filePaths) {
    const key = filePath.replace(/\\/g, "/").trim();
    if (key.length === 0 || seen.has(key)) {
      continue;
    }

    seen.add(key);
    unique.push(filePath);
  }

  return unique;
}

function dedupeBrowserFiles(files: File[]) {
  const seen = new Set<string>();
  const unique: File[] = [];

  for (const file of files) {
    const key = browserFileIdentity(file);
    if (seen.has(key)) {
      continue;
    }

    seen.add(key);
    unique.push(file);
  }

  return unique;
}

function browserQueueItemId(deviceId: string, file: File) {
  return `${deviceId}::${browserFileIdentity(file)}`;
}

function browserFileIdentity(file: File) {
  return `${file.name}::${file.size}::${file.lastModified}`;
}
