/**
 * Mock data used by the initial desktop UI.
 *
 * The dataset is realistic enough to exercise all page states before the
 * backend event bridge is connected. The component tree expects this shape and
 * can later consume live Tauri data with the same interface.
 */

import { HistoryRecord, KindleDeviceView, UploadQueueItem } from "../types";

export const mockDevices: KindleDeviceView[] = [
  {
    id: "pw-signature-11",
    name: "Kindle Paperwhite Signature",
    model: "Paperwhite 11th Gen",
    firmware: "5.17.1.0.3",
    connection: "USB",
    status: "ready",
    uploadAvailable: true,
    batteryLevel: 82,
    storageUsedGb: 5.7,
    storageTotalGb: 32,
    mountPath: "/Volumes/Kindle",
    supportedFormats: ["AZW3", "MOBI", "PDF", "EPUB"],
    lastSeenLabel: "刚刚挂载",
  },
];

export const mockUploadQueue: UploadQueueItem[] = [
  {
    id: "queue-1",
    title: "The Pragmatic Reader",
    author: "Dana K. Rowan",
    sourceFormat: "EPUB",
    targetFormat: "AZW3",
    sizeMb: 8.6,
    convertProgress: 100,
    uploadProgress: 72,
    stage: "uploading",
    destinationPath: undefined,
  },
  {
    id: "queue-2",
    title: "Distributed Systems Field Notes",
    author: "A. Chen",
    sourceFormat: "PDF",
    targetFormat: "AZW3",
    sizeMb: 14.2,
    convertProgress: 44,
    uploadProgress: 0,
    stage: "converting",
    destinationPath: undefined,
  },
];

export const mockHistory: HistoryRecord[] = [
  {
    id: "history-1",
    title: "Building Reliable Systems",
    deviceName: "Kindle Paperwhite Signature",
    connection: "USB",
    outputFormat: "AZW3",
    transferredAt: "今天 · 14:08",
    durationLabel: "38秒",
    sizeLabel: "6.8 MB",
    status: "success",
  },
  {
    id: "history-4",
    title: "Small Team Product Playbook",
    deviceName: "Kindle Paperwhite Signature",
    connection: "USB",
    outputFormat: "AZW3",
    transferredAt: "昨天 · 18:42",
    durationLabel: "47秒",
    sizeLabel: "9.4 MB",
    status: "success",
  },
];
