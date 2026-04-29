/**
 * Thin bridge around the Tauri desktop APIs.
 *
 * The rest of the React tree talks to this module instead of importing Tauri
 * APIs directly, which keeps runtime detection and fallback behavior localized.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
import type {
  DeleteKindleBookResult,
  FrontendStateSnapshot,
  KindleLibraryBook,
} from "../types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export interface NativeDropPayload {
  paths: string[];
}

export function isTauriRuntime() {
  return typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined;
}

export async function getAppState() {
  return invoke<FrontendStateSnapshot>("get_app_state");
}

export async function refreshDevices() {
  return invoke<FrontendStateSnapshot>("refresh_devices");
}

export async function queueUploadFiles(deviceId: string, filePaths: string[]) {
  return invoke<FrontendStateSnapshot>("queue_upload_files", {
    request: {
      deviceId,
      filePaths,
    },
  });
}

export async function startUpload(deviceId: string) {
  return invoke<FrontendStateSnapshot>("start_upload", {
    request: {
      deviceId,
    },
  });
}

export async function listKindleBooks(deviceId: string) {
  return invoke<KindleLibraryBook[]>("list_kindle_books", {
    request: {
      deviceId,
    },
  });
}

export async function deleteKindleBook(deviceId: string, bookId: string) {
  return invoke<DeleteKindleBookResult>("delete_kindle_book", {
    request: {
      deviceId,
      bookId,
    },
  });
}

export async function renameKindleBook(
  deviceId: string,
  bookId: string,
  title: string,
) {
  return invoke<KindleLibraryBook>("rename_kindle_book", {
    request: {
      deviceId,
      bookId,
      title,
    },
  });
}

export async function openUploadFilePicker() {
  const selected = await open({
    multiple: true,
    directory: false,
    title: t("dialog.uploadTitle"),
    filters: [
      {
        name: t("dialog.ebookFilter"),
        extensions: ["epub", "mobi", "azw3", "pdf"],
      },
    ],
  });

  if (selected === null) {
    return [];
  }

  return Array.isArray(selected) ? selected : [selected];
}

export async function listenToStateChanges(
  onStateChanged: (snapshot: FrontendStateSnapshot) => void,
) {
  return listen<FrontendStateSnapshot>("kindle://state", (event) => {
    onStateChanged(event.payload);
  });
}

export async function listenToNativeDrop(
  onDrop: (payload: NativeDropPayload) => void,
  onHoverChange?: (isHovering: boolean) => void,
) {
  const currentWindow = getCurrentWindow();

  return currentWindow.onDragDropEvent((event) => {
    if (event.payload.type === "over" || event.payload.type === "enter") {
      onHoverChange?.(true);
      return;
    }

    if (event.payload.type === "leave") {
      onHoverChange?.(false);
      return;
    }

    if (event.payload.type === "drop") {
      onHoverChange?.(false);
      onDrop({
        paths: event.payload.paths,
      });
    }
  });
}
