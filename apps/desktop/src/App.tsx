import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  discardChanges,
  editValues,
  ErrorCodes,
  isApiError,
  openFile,
  pendingChanges,
  redo,
  restoreBackup,
  save,
  undo,
  type Change,
  type HistoryResult,
  type HistoryState,
  type OpenInfo,
} from "@/api/commands";
import { TooltipProvider } from "@/components/ui/tooltip";
import { EditorView, type EditorHandle } from "@/editor";
import { AboutDialog } from "./shell/AboutDialog";
import { BackgroundPattern } from "./shell/BackgroundPattern";
import { BackupsDialog } from "./shell/BackupsDialog";
import { BottomBar } from "./shell/BottomBar";
import { ConfirmDiscardDialog } from "./shell/ConfirmDiscardDialog";
import { useNotify, useSettings } from "./shell/context";
import { baseName, exportFileName } from "./shell/engine";
import { describeError } from "./shell/errors";
import { applyJson, exportJson, exportJsonFile } from "./shell/exportApi";
import { Header } from "./shell/Header";
import { JsonDialog, type ApplyResult, type JsonState } from "./shell/JsonDialog";
import { OpeningOverlay } from "./shell/OpeningOverlay";
import { PasswordDialog } from "./shell/PasswordDialog";
import { addRecent, loadRecent, saveRecent, type RecentFile } from "./shell/recent";
import { SettingsDialog } from "./shell/SettingsDialog";
import { browserStorage } from "./shell/settings";
import { firstListSearch, isTextEditing, shortcutFor } from "./shell/shortcuts";
import { StartScreen } from "./shell/StartScreen";
import { Toasts } from "./shell/Toasts";

/** Opening a file shows the loading cover only when it takes longer than this. */
const OPEN_COVER_DELAY_MS = 300;

const NO_HISTORY: HistoryState = { canUndo: false, canRedo: false };

interface PasswordRequest {
  path: string;
  wrong: boolean;
}

function App() {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const { notify } = useNotify();

  const [file, setFile] = useState<OpenInfo | null>(null);
  const [changeCount, setChangeCount] = useState(0);
  const [invalid, setInvalid] = useState(false);
  /** Bumped to make the editor read the document from the host again. */
  const [editorKey, setEditorKey] = useState(0);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [recent, setRecent] = useState<RecentFile[]>(() => loadRecent(browserStorage()));
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [backupsOpen, setBackupsOpen] = useState(false);
  /** Name of the file that is taking a while to open (the loading cover), else null. */
  const [opening, setOpening] = useState<string | null>(null);
  const [history, setHistory] = useState<HistoryState>(NO_HISTORY);
  const editorRef = useRef<EditorHandle>(null);
  /** Undo and redo run one after the other, so a held or fast key press keeps its order. */
  const historyQueue = useRef<Promise<void>>(Promise.resolve());
  const [passwordRequest, setPasswordRequest] = useState<PasswordRequest | null>(null);
  const [pendingOpen, setPendingOpen] = useState<string | null>(null);
  const [jsonOpen, setJsonOpen] = useState(false);
  const [jsonState, setJsonState] = useState<JsonState>({ status: "loading" });
  /** The JSON editor holds edits that are not applied yet. */
  const [jsonDirty, setJsonDirty] = useState(false);
  const [exporting, setExporting] = useState(false);
  /** The window's close button was pressed while there were unsaved changes. */
  const [closeAsked, setCloseAsked] = useState(false);

  const notifyError = useCallback(
    (title: string, error: unknown) => {
      const { reason, fix } = describeError(error);
      notify({
        kind: "error",
        title,
        detail: t("toast.errorDetail", { reason, fix }).trim(),
      });
    },
    [notify, t],
  );

  const rememberRecent = useCallback(
    (info: OpenInfo) => {
      if (!settings.showRecent) return;
      setRecent((list) => {
        const next = addRecent(list, {
          path: info.path,
          fileName: info.fileName,
          openedAt: Date.now(),
        });
        saveRecent(next, browserStorage());
        return next;
      });
    },
    [settings.showRecent],
  );

  const openPath = useCallback(
    async (path: string, password?: string) => {
      setBusy(true);
      const cover = window.setTimeout(() => setOpening(baseName(path)), OPEN_COVER_DELAY_MS);
      try {
        const info = await openFile(path, password);
        setFile(info);
        setChangeCount(0);
        setHistory(NO_HISTORY);
        setInvalid(false);
        setEditorKey((key) => key + 1);
        setPasswordRequest(null);
        rememberRecent(info);
        notify({
          kind: "opened",
          title: t("bell.opened", { name: info.fileName }),
          detail: info.path,
          toast: false,
        });
      } catch (error) {
        if (
          isApiError(error) &&
          (error.code === ErrorCodes.passwordRequired || error.code === ErrorCodes.wrongPassword)
        ) {
          setPasswordRequest({ path, wrong: error.code === ErrorCodes.wrongPassword });
        } else {
          setPasswordRequest(null);
          notifyError(t("toast.openFailed"), error);
        }
      } finally {
        window.clearTimeout(cover);
        setOpening(null);
        setBusy(false);
      }
    },
    [notify, notifyError, rememberRecent, t],
  );

  /**
   * Opens `path`; with unsaved changes it asks first. Ignored while another action runs or the
   * JSON editor is open (its text belongs to the open file).
   */
  const requestOpen = useCallback(
    (path: string) => {
      if (busy || jsonOpen) return;
      if (file && changeCount > 0) {
        setPendingOpen(path);
        return;
      }
      void openPath(path);
    },
    [busy, changeCount, file, jsonOpen, openPath],
  );

  const chooseAndOpen = useCallback(async () => {
    if (busy) return;
    try {
      const picked = await openDialog({ multiple: false, directory: false, title: t("dialog.openTitle") });
      if (typeof picked === "string") requestOpen(picked);
    } catch (error) {
      notifyError(t("toast.openFailed"), error);
    }
  }, [busy, notifyError, requestOpen, t]);

  // Drag and drop of a file onto the window (the host reports real paths).
  const openPathRef = useRef(requestOpen);
  useEffect(() => {
    openPathRef.current = requestOpen;
  }, [requestOpen]);

  useEffect(() => {
    const handle: { stop: (() => void) | null } = { stop: null };
    let disposed = false;
    try {
      void getCurrentWebview()
        .onDragDropEvent((event) => {
          const payload = event.payload;
          if (payload.type === "enter") {
            setDragging(true);
          } else if (payload.type === "leave") {
            setDragging(false);
          } else if (payload.type === "drop") {
            setDragging(false);
            const first = payload.paths[0];
            if (first) openPathRef.current(first);
          }
        })
        .then((stop) => {
          if (disposed) stop();
          else handle.stop = stop;
        })
        .catch(() => undefined);
    } catch {
      // Not running inside the desktop shell (for example plain `vite`): no drag and drop.
    }
    return () => {
      disposed = true;
      if (handle.stop) handle.stop();
    };
  }, []);

  // Closing the window with unsaved changes (or JSON edits not applied) asks first. While this
  // handler is registered the host waits for it: without `preventDefault()` the window is destroyed
  // right away.
  const hasChangesRef = useRef(false);
  useEffect(() => {
    hasChangesRef.current = file !== null && (changeCount > 0 || jsonDirty);
  }, [file, changeCount, jsonDirty]);

  useEffect(() => {
    const handle: { stop: (() => void) | null } = { stop: null };
    let disposed = false;
    try {
      void getCurrentWindow()
        .onCloseRequested((event) => {
          if (!hasChangesRef.current) return;
          event.preventDefault();
          setCloseAsked(true);
        })
        .then((stop) => {
          if (disposed) stop();
          else handle.stop = stop;
        })
        .catch(() => undefined);
    } catch {
      // Not running inside the desktop shell: nothing to guard.
    }
    return () => {
      disposed = true;
      if (handle.stop) handle.stop();
    };
  }, []);

  const closeWithoutSaving = useCallback(() => {
    setCloseAsked(false);
    getCurrentWindow()
      .destroy()
      .catch((error: unknown) => notifyError(t("toast.closeFailed"), error));
  }, [notifyError, t]);

  const afterSave = useCallback(
    async (target: string, overwrite: boolean) => {
      setBusy(true);
      try {
        const result = await save(target, overwrite);
        if (!result.verified) {
          notify({
            kind: "error",
            title: t("toast.verifyFailed"),
            detail: result.writtenPath,
          });
          return;
        }
        setFile((current) =>
          current
            ? { ...current, path: result.writtenPath, fileName: baseName(result.writtenPath) }
            : current,
        );
        setChangeCount(0);
        setHistory(NO_HISTORY);
        setEditorKey((key) => key + 1);
        const written = result.writtenPath;
        notify({
          kind: "saved",
          title: result.backupPath ? t("toast.savedTitle") : t("toast.savedNoBackupTitle"),
          detail: t("toast.savedBody", { path: written }),
          action: {
            label: t("toast.showInFolder"),
            run: () => {
              revealItemInDir(written).catch((error: unknown) =>
                notifyError(t("toast.showInFolder"), error),
              );
            },
          },
        });
        if (result.backupPath) {
          notify({
            kind: "info",
            title: t("bell.backup", { path: result.backupPath }),
            toast: false,
          });
        }
      } catch (error) {
        notifyError(t("toast.saveFailed"), error);
      } finally {
        setBusy(false);
      }
    },
    [notify, notifyError, t],
  );

  const saveAsNew = useCallback(async () => {
    if (!file) return;
    try {
      const target = await saveDialog({
        title: t("dialog.saveTitle"),
        defaultPath: file.path,
      });
      // The system dialog has already asked about replacing an existing file.
      if (typeof target === "string") await afterSave(target, true);
    } catch (error) {
      notifyError(t("toast.saveFailed"), error);
    }
  }, [afterSave, file, notifyError, t]);

  const overwriteOriginal = useCallback(async () => {
    if (!file) return;
    await afterSave(file.path, true);
  }, [afterSave, file]);

  const undoAll = useCallback(async () => {
    setBusy(true);
    try {
      await discardChanges();
      setChangeCount(0);
      setEditorKey((key) => key + 1);
      notify({ kind: "info", title: t("bell.undone"), toast: false });
    } catch (error) {
      notifyError(t("toast.discardFailed"), error);
    } finally {
      setBusy(false);
    }
  }, [notify, notifyError, t]);

  /**
   * Shows what an undo, a redo or a "Set all" did. The cards show the values it changed in place
   * (scroll position, searches and open groups stay); a step with very many values makes them read
   * everything again.
   */
  const showHistoryResult = useCallback((result: HistoryResult) => {
    if (result.reload) setEditorKey((key) => key + 1);
    else editorRef.current?.applyHistory(result.fields);
    setChangeCount(result.pending);
    setHistory({ canUndo: result.canUndo, canRedo: result.canRedo });
  }, []);

  /** Undo or redo the last action. */
  const stepHistory = useCallback(
    (kind: "undo" | "redo") => {
      historyQueue.current = historyQueue.current.then(async () => {
        try {
          showHistoryResult(await (kind === "undo" ? undo() : redo()));
        } catch (error) {
          notifyError(t(kind === "undo" ? "toast.undoFailed" : "toast.redoFailed"), error);
        }
      });
    },
    [notifyError, showHistoryResult, t],
  );

  /**
   * A list's "Set all": every value in `paths` to `text` as one undo step, in line with undo and
   * redo. All or nothing: a value the host refuses (out of range) leaves every value as it was.
   */
  const editMany = useCallback(
    (paths: string[], text: string): Promise<boolean> => {
      const step = historyQueue.current.then(async () => {
        try {
          showHistoryResult(await editValues(paths, text));
          notify({
            kind: "info",
            title: t("bell.setAll", { count: paths.length, value: text }),
            toast: false,
          });
          return true;
        } catch (error) {
          notifyError(t("toast.setAllFailed"), error);
          return false;
        }
      });
      historyQueue.current = step.then(() => undefined);
      return step;
    },
    [notify, notifyError, showHistoryResult, t],
  );

  /**
   * Puts a backup back in place of the open file. The host backs up the current file first and
   * opens the file again, so the cards start over: no pending changes, no undo history.
   */
  const restoreFrom = useCallback(
    async (name: string, when: string) => {
      setBusy(true);
      try {
        const info = await restoreBackup(name);
        setFile(info);
        setChangeCount(0);
        setHistory(NO_HISTORY);
        setInvalid(false);
        setEditorKey((key) => key + 1);
        setBackupsOpen(false);
        notify({
          kind: "saved",
          title: t("toast.restoredTitle"),
          detail: t("toast.restoredBody", { path: info.path, time: when }),
        });
      } catch (error) {
        notifyError(t("toast.restoreFailed"), error);
      } finally {
        setBusy(false);
      }
    },
    [notify, notifyError, t],
  );

  const loadChanges = useCallback(async (): Promise<Change[]> => {
    try {
      return await pendingChanges();
    } catch (error) {
      notifyError(t("toast.changesFailed"), error);
      return [];
    }
  }, [notifyError, t]);

  const openJson = useCallback(() => {
    setJsonState({ status: "loading" });
    setJsonOpen(true);
    exportJson()
      .then((text) => setJsonState({ status: "ready", text }))
      .catch((error: unknown) => {
        const { reason, fix } = describeError(error);
        setJsonState({
          status: "error",
          message: `${t("json.failed")} ${t("toast.errorDetail", { reason, fix })}`.trim(),
        });
      });
  }, [t]);

  /**
   * Applies the edited JSON text. On success the cards read the document again, the editor gets
   * the fresh text and a toast counts the changes; a refusal goes back to the editor to show.
   */
  const applyJsonText = useCallback(
    async (text: string): Promise<ApplyResult> => {
      let count: number;
      try {
        count = await applyJson(text);
      } catch (error) {
        return { ok: false, error };
      }
      setEditorKey((key) => key + 1);
      notify({
        kind: "saved",
        title: count === 0 ? t("toast.jsonNoChanges") : t("toast.jsonApplied", { count }),
        skipBell: count === 0,
      });
      try {
        setJsonState({ status: "ready", text: await exportJson() });
      } catch (error) {
        const { reason, fix } = describeError(error);
        setJsonState({
          status: "error",
          message: `${t("json.failed")} ${t("toast.errorDetail", { reason, fix })}`.trim(),
        });
      }
      return { ok: true };
    },
    [notify, t],
  );

  /** Saves the JSON view as a UTF-8 `.json` file at a place the user picks. */
  const exportJsonText = useCallback(async () => {
    if (!file) return;
    setExporting(true);
    try {
      const target = await saveDialog({
        title: t("dialog.exportTitle"),
        defaultPath: exportFileName(file.fileName),
        filters: [{ name: t("dialog.jsonFiles"), extensions: ["json"] }],
      });
      if (typeof target !== "string") return;
      const written = await exportJsonFile(target);
      notify({
        kind: "saved",
        title: t("toast.exportedTitle"),
        detail: written,
        action: {
          label: t("toast.showInFolder"),
          run: () => {
            revealItemInDir(written).catch((error: unknown) =>
              notifyError(t("toast.showInFolder"), error),
            );
          },
        },
      });
    } catch (error) {
      notifyError(t("toast.exportFailed"), error);
    } finally {
      setExporting(false);
    }
  }, [file, notify, notifyError, t]);

  const readOnly = file !== null && !file.editable;

  // Keyboard shortcuts. The listener is registered once and reads the latest state from a ref.
  const shortcutHandler = useRef<(event: KeyboardEvent) => void>(() => undefined);
  useEffect(() => {
    shortcutHandler.current = (event) => {
      const action = shortcutFor(event);
      if (action === null) return;
      // Typing owns Ctrl+Z and Ctrl+Y in a text box and in the JSON editor. A key the editor
      // already handled (its own Ctrl+F) is left alone too.
      const target = event.target instanceof Element ? event.target : null;
      if ((action === "undo" || action === "redo") && isTextEditing(target)) return;
      if (event.defaultPrevented) return;
      event.preventDefault();
      // Nothing happens behind a dialog or an open menu, or while another action runs.
      const modal =
        settingsOpen ||
        aboutOpen ||
        backupsOpen ||
        jsonOpen ||
        passwordRequest !== null ||
        pendingOpen !== null ||
        closeAsked ||
        document.querySelector('[role="menu"]') !== null;
      if (modal || busy) return;
      if (action === "open") {
        if (!event.repeat) void chooseAndOpen();
        return;
      }
      if (file === null) return;
      if (action === "undo" || action === "redo") {
        stepHistory(action);
      } else if (action === "find") {
        const root = document.querySelector("main");
        const box = root ? firstListSearch(root) : null;
        box?.focus();
        box?.select();
      } else if (!event.repeat) {
        // Save and Save as: the same checks as the buttons of the bottom bar.
        if (readOnly || invalid) {
          notify({
            kind: "info",
            title: t(readOnly ? "bottom.readOnlyTip" : "bottom.invalidTip"),
            skipBell: true,
          });
        } else if (action === "saveAs" || settings.saveDefault === "saveAs") {
          void saveAsNew();
        } else {
          void overwriteOriginal();
        }
      }
    };
  });
  useEffect(() => {
    const listener = (event: KeyboardEvent) => shortcutHandler.current(event);
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, []);

  return (
    <TooltipProvider>
      <div className="relative flex h-screen flex-col bg-background text-foreground">
        <BackgroundPattern />
        <Header
          file={file}
          onOpenFile={() => void chooseAndOpen()}
          onOpenSettings={() => setSettingsOpen(true)}
        />

        {readOnly ? (
          <div
            role="status"
            className="relative z-10 flex h-8 shrink-0 items-center gap-2 border-b border-warning/40 bg-chrome px-3 text-xs font-medium text-warning"
          >
            <TriangleAlert className="size-3.5 shrink-0" />
            {t("banner.readOnly")}
          </div>
        ) : null}

        <div className="relative flex min-h-0 flex-1 flex-col">
          {file ? (
            <main
              className="dense-scroll relative z-10 min-h-0 flex-1 overflow-y-auto"
              inert={opening !== null}
            >
              <EditorView
                key={editorKey}
                ref={editorRef}
                readOnly={readOnly}
                onChangesChanged={setChangeCount}
                onInvalidChanged={setInvalid}
                onHistoryChanged={setHistory}
                onEditMany={editMany}
              />
            </main>
          ) : (
            <StartScreen
              dragging={dragging}
              recent={recent}
              onOpenFile={() => void chooseAndOpen()}
              onOpenRecent={requestOpen}
            />
          )}
          {opening !== null ? <OpeningOverlay name={opening} /> : null}
        </div>

        {file ? (
          <BottomBar
            changeCount={changeCount}
            readOnly={readOnly}
            invalid={invalid}
            busy={busy}
            saveDefault={settings.saveDefault}
            canUndo={history.canUndo}
            canRedo={history.canRedo}
            loadChanges={loadChanges}
            onUndo={() => stepHistory("undo")}
            onRedo={() => stepHistory("redo")}
            onUndoAll={() => void undoAll()}
            onOpenJson={openJson}
            onOpenBackups={() => setBackupsOpen(true)}
            onSaveAs={() => void saveAsNew()}
            onOverwrite={() => void overwriteOriginal()}
          />
        ) : null}

        <SettingsDialog
          open={settingsOpen}
          onOpenChange={setSettingsOpen}
          onOpenAbout={() => {
            setSettingsOpen(false);
            setAboutOpen(true);
          }}
        />
        <AboutDialog open={aboutOpen} onOpenChange={setAboutOpen} />
        {file && backupsOpen ? (
          <BackupsDialog
            fileName={file.fileName}
            changeCount={changeCount}
            busy={busy}
            onClose={() => setBackupsOpen(false)}
            onRestore={restoreFrom}
          />
        ) : null}
        {jsonOpen ? (
          <JsonDialog
            state={jsonState}
            readOnly={readOnly}
            onClose={() => setJsonOpen(false)}
            onExport={() => void exportJsonText()}
            exporting={exporting}
            onApply={applyJsonText}
            onDirtyChange={setJsonDirty}
          />
        ) : null}
        {passwordRequest ? (
          <PasswordDialog
            name={baseName(passwordRequest.path)}
            wrong={passwordRequest.wrong}
            onSubmit={(password) => void openPath(passwordRequest.path, password)}
            onCancel={() => setPasswordRequest(null)}
          />
        ) : null}
        {pendingOpen ? (
          <ConfirmDiscardDialog
            count={changeCount}
            onCancel={() => setPendingOpen(null)}
            onDiscard={() => {
              const path = pendingOpen;
              setPendingOpen(null);
              void openPath(path);
            }}
          />
        ) : null}
        {closeAsked ? (
          <ConfirmDiscardDialog
            reason="close"
            // JSON edits that are not applied count as one more change.
            count={changeCount + (jsonDirty ? 1 : 0)}
            onCancel={() => setCloseAsked(false)}
            onDiscard={closeWithoutSaving}
          />
        ) : null}
        <Toasts />
      </div>
    </TooltipProvider>
  );
}

export default App;
