/**
 * Shared view models used by the desktop UI.
 *
 * These types mirror the information the Rust backend will eventually expose
 * through Tauri commands and events. Keeping them centralized avoids prop drift
 * as the frontend grows.
 */

export type ConnectionKind = "USB";
export type AppRuntimeMode = "tauri" | "browser";

export type DeviceStatus = "ready" | "syncing" | "idle";

export type QueueStage =
  | "queued"
  | "converting"
  | "uploading"
  | "verifying"
  | "done"
  | "failed";

export type HistoryStatus = "success" | "partial" | "failed";

export interface KindleDeviceView {
  id: string;
  name: string;
  model: string;
  firmware: string;
  connection: ConnectionKind;
  status: DeviceStatus;
  uploadAvailable: boolean;
  batteryLevel: number;
  storageUsedGb: number;
  storageTotalGb: number;
  ipAddress?: string;
  mountPath?: string;
  supportedFormats: string[];
  lastSeenLabel: string;
  endpoint?: string;
}

export interface UploadQueueItem {
  id: string;
  title: string;
  author: string;
  sourceFormat: string;
  targetFormat: string;
  sizeMb: number;
  convertProgress: number;
  uploadProgress: number;
  stage: QueueStage;
  destinationPath?: string;
}

export interface HistoryRecord {
  id: string;
  title: string;
  deviceName: string;
  connection: ConnectionKind;
  outputFormat: string;
  transferredAt: string;
  durationLabel: string;
  sizeLabel: string;
  status: HistoryStatus;
}

export interface KindleLibraryBook {
  id: string;
  title: string;
  format: string;
  sizeMb: number;
  sizeLabel: string;
  modifiedAt: string;
  path: string;
  relativePath: string;
  sidecarPath?: string;
}

export interface DeleteKindleBookResult {
  deletedBookId: string;
  deletedTitle: string;
  removedPaths: string[];
}

export interface FrontendStateSnapshot {
  devices: KindleDeviceView[];
  uploadQueue: UploadQueueItem[];
  history: HistoryRecord[];
}
