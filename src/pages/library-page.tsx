/**
 * Kindle on-device library page.
 */

import { useEffect, useState } from "react";
import { t } from "../i18n";
import {
  AppRuntimeMode,
  KindleDeviceView,
  KindleLibraryBook,
} from "../types";

interface LibraryPageProps {
  devices: KindleDeviceView[];
  runtimeMode: AppRuntimeMode;
  selectedDeviceId: string;
  onSelectDevice: (deviceId: string) => void;
  onLoadBooks: (deviceId: string) => Promise<KindleLibraryBook[]>;
  onDeleteBook: (deviceId: string, bookId: string) => Promise<void>;
  onRenameBook: (
    deviceId: string,
    bookId: string,
    title: string,
  ) => Promise<KindleLibraryBook | null>;
}

export function LibraryPage({
  devices,
  runtimeMode,
  selectedDeviceId,
  onSelectDevice,
  onLoadBooks,
  onDeleteBook,
  onRenameBook,
}: LibraryPageProps) {
  const selectedDevice =
    devices.find((device) => device.id === selectedDeviceId) ?? devices[0] ?? null;
  const [books, setBooks] = useState<KindleLibraryBook[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [deletingBookId, setDeletingBookId] = useState<string | null>(null);
  const [editingBookId, setEditingBookId] = useState<string | null>(null);
  const [renamingBookId, setRenamingBookId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  async function refreshBooks(deviceId = selectedDevice?.id ?? "") {
    if (runtimeMode !== "tauri" || deviceId.trim().length === 0) {
      setBooks([]);
      return;
    }

    setIsLoading(true);
    setErrorMessage(null);
    try {
      setBooks(await onLoadBooks(deviceId));
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setIsLoading(false);
    }
  }

  useEffect(() => {
    void refreshBooks(selectedDevice?.id ?? "");
  }, [runtimeMode, selectedDevice?.id]);

  useEffect(() => {
    setEditingBookId(null);
    setRenameDraft("");
  }, [selectedDevice?.id]);

  function handleStartRename(book: KindleLibraryBook) {
    setErrorMessage(null);
    setEditingBookId(book.id);
    setRenameDraft(book.title);
  }

  function handleCancelRename() {
    setEditingBookId(null);
    setRenameDraft("");
  }

  async function handleRename(book: KindleLibraryBook) {
    if (!selectedDevice) {
      return;
    }

    const nextTitle = renameDraft.trim();
    if (nextTitle.length === 0) {
      setErrorMessage(t("library.renameEmpty"));
      return;
    }

    if (nextTitle === book.title) {
      handleCancelRename();
      return;
    }

    setRenamingBookId(book.id);
    setErrorMessage(null);
    try {
      await onRenameBook(selectedDevice.id, book.id, nextTitle);
      handleCancelRename();
      await refreshBooks(selectedDevice.id);
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setRenamingBookId(null);
    }
  }

  async function handleDelete(book: KindleLibraryBook) {
    if (!selectedDevice) {
      return;
    }

    const confirmed = window.confirm(
      t("library.deleteConfirm").replace("{title}", book.title),
    );
    if (!confirmed) {
      return;
    }

    setDeletingBookId(book.id);
    setErrorMessage(null);
    try {
      await onDeleteBook(selectedDevice.id, book.id);
      await refreshBooks(selectedDevice.id);
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setDeletingBookId(null);
    }
  }

  return (
    <div className="space-y-6">
      <section className="panel p-6">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <p className="text-[11px] uppercase tracking-[0.2em] text-slate-500">
              {t("library.currentDevice")}
            </p>
            <p className="mt-2 font-display text-2xl text-slate-50">
              {selectedDevice ? selectedDevice.name : t("library.noDevice")}
            </p>
          </div>
          <button
            type="button"
            disabled={!selectedDevice || isLoading}
            onClick={() => void refreshBooks()}
            className="rounded-full border border-white/10 bg-white/[0.04] px-5 py-2.5 text-sm text-slate-100 transition hover:bg-white/[0.08] disabled:cursor-not-allowed disabled:text-slate-500"
          >
            {isLoading ? t("library.refreshing") : t("library.refresh")}
          </button>
        </div>

        {devices.length > 1 ? (
          <div className="mt-5 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
            {devices.map((device) => (
              <button
                key={device.id}
                type="button"
                onClick={() => onSelectDevice(device.id)}
                className={`rounded-2xl border px-4 py-3 text-left transition ${selectedDevice?.id === device.id ? "border-cyan-300/35 bg-cyan-400/[0.08] text-slate-50" : "border-white/8 bg-white/[0.03] text-slate-400 hover:border-white/14 hover:text-slate-200"}`}
              >
                <p className="text-sm font-medium">{device.name}</p>
                <p className="mt-1 text-xs uppercase tracking-[0.18em]">
                  {device.connection}
                </p>
              </button>
            ))}
          </div>
        ) : null}

        {errorMessage ? (
          <div className="mt-5 rounded-[20px] border border-red-300/15 bg-red-400/[0.08] px-4 py-3 text-sm leading-6 text-red-100">
            {errorMessage}
          </div>
        ) : null}
      </section>

      <section className="panel p-6">
        <div className="flex items-center justify-between gap-4">
          <div>
            <p className="text-[11px] uppercase tracking-[0.2em] text-slate-500">
              {t("library.bookList")}
            </p>
            <p className="mt-2 font-display text-2xl text-slate-50">
              {books.length} {t("library.booksUnit")}
            </p>
          </div>
        </div>

        <div className="mt-6 space-y-3">
          {books.length === 0 ? (
            <div className="rounded-[24px] border border-white/8 bg-white/[0.03] p-6 text-sm text-slate-400">
              {isLoading ? t("library.loading") : t("library.empty")}
            </div>
          ) : (
            books.map((book) => (
              <article
                key={book.id}
                className="grid gap-4 rounded-[24px] border border-white/8 bg-white/[0.03] p-5 transition hover:bg-white/[0.045] lg:grid-cols-[1fr_auto]"
              >
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    {editingBookId === book.id ? (
                      <form
                        className="flex min-w-[min(520px,100%)] flex-1 flex-wrap items-center gap-2"
                        onSubmit={(event) => {
                          event.preventDefault();
                          void handleRename(book);
                        }}
                      >
                        <input
                          autoFocus
                          value={renameDraft}
                          onChange={(event) => setRenameDraft(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === "Escape") {
                              handleCancelRename();
                            }
                          }}
                          placeholder={t("library.renamePlaceholder")}
                          className="min-w-0 flex-1 rounded-full border border-cyan-300/20 bg-slate-950/65 px-4 py-2 text-sm text-slate-50 outline-none transition placeholder:text-slate-600 focus:border-cyan-300/50"
                        />
                        <button
                          type="submit"
                          disabled={renamingBookId === book.id}
                          className="rounded-full bg-cyan-300 px-4 py-2 text-sm font-medium text-slate-950 transition hover:bg-cyan-200 disabled:cursor-not-allowed disabled:opacity-55"
                        >
                          {renamingBookId === book.id
                            ? t("library.renaming")
                            : t("library.renameSave")}
                        </button>
                        <button
                          type="button"
                          disabled={renamingBookId === book.id}
                          onClick={handleCancelRename}
                          className="rounded-full border border-white/10 bg-white/[0.03] px-4 py-2 text-sm text-slate-300 transition hover:bg-white/[0.07] disabled:cursor-not-allowed disabled:opacity-55"
                        >
                          {t("library.renameCancel")}
                        </button>
                      </form>
                    ) : (
                      <p className="truncate font-medium text-slate-100">{book.title}</p>
                    )}
                    <span className="rounded-full border border-white/8 bg-white/[0.04] px-2.5 py-1 text-[11px] uppercase tracking-[0.14em] text-slate-300">
                      {book.format}
                    </span>
                  </div>
                  <p className="mt-2 break-all text-sm leading-6 text-slate-400">
                    {book.relativePath}
                  </p>
                  <div className="mt-3 flex flex-wrap gap-2 text-xs text-slate-500">
                    <span>{book.sizeLabel}</span>
                    <span>{book.modifiedAt}</span>
                  </div>
                </div>

                <div className="flex flex-wrap gap-2 self-start">
                  <button
                    type="button"
                    disabled={renamingBookId === book.id || deletingBookId === book.id}
                    onClick={() => handleStartRename(book)}
                    className="rounded-full border border-white/10 bg-white/[0.04] px-4 py-2 text-sm text-slate-100 transition hover:bg-white/[0.08] disabled:cursor-not-allowed disabled:text-slate-500"
                  >
                    {t("library.rename")}
                  </button>
                  <button
                    type="button"
                    disabled={deletingBookId === book.id || renamingBookId === book.id}
                    onClick={() => void handleDelete(book)}
                    className="rounded-full border border-red-300/20 bg-red-400/[0.08] px-4 py-2 text-sm text-red-100 transition hover:bg-red-400/[0.14] disabled:cursor-not-allowed disabled:text-red-100/45"
                  >
                    {deletingBookId === book.id ? t("library.deleting") : t("library.delete")}
                  </button>
                </div>
              </article>
            ))
          )}
        </div>
      </section>
    </div>
  );
}
