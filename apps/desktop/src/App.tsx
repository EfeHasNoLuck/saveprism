import { useCallback, useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { Info, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  closeFile,
  copyCode,
  discardChanges,
  editValues,
  ErrorCodes,
  fileReport,
  isApiError,
  openCode,
  openFile,
  openFileWithGameFolder,
  pendingChanges,
  redo,
  restoreBackup,
  save,
  takePendingFile,
  undo,
  type Change,
  type HistoryResult,
  type HistoryState,
  type OpenInfo,
} from "@/api/commands";
import { TooltipProvider } from "@/components/ui/tooltip";
import { EditorView, type EditorHandle } from "@/editor";
import { countChangeKinds } from "@/editor/helpers";
import { BackgroundPattern } from "./shell/BackgroundPattern";
import { BackupsDialog } from "./shell/BackupsDialog";
import { BottomBar, type LabeledChange } from "./shell/BottomBar";
import { copyCodePlace, pastedText, saveAsDefaultPath } from "./shell/code";
import { CodeTextDialog, PasteCodeDialog, type OpenCodeResult } from "./shell/CodeDialogs";
import { CompareDialog } from "./shell/CompareDialog";
import { ConfirmDiscardDialog } from "./shell/ConfirmDiscardDialog";
import { useNotify, useSettings, type ToastAction } from "./shell/context";
import { baseName, engineLabel, exportFileName } from "./shell/engine";
import { describeError } from "./shell/errors";
import { applyJson, exportJson, exportJsonFile } from "./shell/exportApi";
import { buildFileReport } from "./shell/fileReport";
import { Header } from "./shell/Header";
import { JsonDialog, type ApplyResult, type JsonState } from "./shell/JsonDialog";
import { OpeningOverlay } from "./shell/OpeningOverlay";
import { gameFolder } from "./shell/names";
import {
  LAST_SEEN_VERSION_KEY,
  readStored,
  TOUR_SEEN_KEY,
  tourSeen,
  whatsNewDecision,
  writeStored,
} from "./shell/onboarding";
import { PasswordDialog } from "./shell/PasswordDialog";
import { SaveLocationsDialog } from "./shell/SaveLocationsDialog";
import { SettingsPage } from "./shell/SettingsPage";
import { firstListSearch, isTextEditing, shortcutFor } from "./shell/shortcuts";
import { StartScreen } from "./shell/StartScreen";
import { Toasts } from "./shell/Toasts";
import { Tour } from "./shell/TourOverlay";
import { WhatsNewDialog } from "./shell/WhatsNewDialog";

/** Opening a file shows the loading cover only when it takes longer than this. */
const OPEN_COVER_DELAY_MS = 300;

const NO_HISTORY: HistoryState = { canUndo: false, canRedo: false };
const NO_CHANGE_KINDS = { added: 0, removed: 0 };

interface PasswordRequest {
  path: string;
  wrong: boolean;
  /** The last try searched a folder the player picked and found no key there. */
  folderTried: boolean;
}

function App() {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const { notify } = useNotify();

  const [file, setFile] = useState<OpenInfo | null>(null);
  const [changeCount, setChangeCount] = useState(0);
  /** Added and removed entries among the pending changes. */
  const [changeKinds, setChangeKinds] = useState(NO_CHANGE_KINDS);
  const [invalid, setInvalid] = useState(false);
  /** Bumped to make the editor read the document from the host again. */
  const [editorKey, setEditorKey] = useState(0);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [tourRunning, setTourRunning] = useState(() => !tourSeen());
  /** The version whose "What's new" dialog is open, else null. */
  const [whatsNewVersion, setWhatsNewVersion] = useState<string | null>(null);
  /** "Show the tour again" asked for the start screen while a file is open. */
  const [tourAfterClose, setTourAfterClose] = useState(false);
  const [backupsOpen, setBackupsOpen] = useState(false);
  /** Path of the second save of a comparison, while its dialog is open. */
  const [comparePath, setComparePath] = useState<string | null>(null);
  const [locationsOpen, setLocationsOpen] = useState(false);
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
  /** "Close file" was pressed while there were unsaved changes. */
  const [closeFileAsked, setCloseFileAsked] = useState(false);
  /** The "Paste code" dialog is open, with the text it starts with. */
  const [pasteRequest, setPasteRequest] = useState<{ text: string } | null>(null);
  /** A copied code the clipboard refused, shown to copy by hand. */
  const [codeText, setCodeText] = useState<string | null>(null);

  const resetChanges = useCallback(() => {
    setChangeCount(0);
    setChangeKinds(NO_CHANGE_KINDS);
  }, []);

  /** The editor read the pending changes: show their count and the added/removed counts. */
  const applyChanges = useCallback((changes: Change[]) => {
    setChangeCount(changes.length);
    setChangeKinds(countChangeKinds(changes));
  }, []);

  const notifyError = useCallback(
    (title: string, error: unknown, action?: ToastAction) => {
      const { reason, fix } = describeError(error, t);
      notify({
        kind: "error",
        title,
        detail: t("toast.errorDetail", { reason, fix }).trim(),
        action,
      });
    },
    [notify, t],
  );

  /**
   * Copies the local report of a file SavePrism does not know (extension, size, first bytes, hash;
   * never the path) so the player can paste it into an issue. Nothing is sent anywhere.
   */
  const copyFileReport = useCallback(
    async (path: string) => {
      let text: string;
      try {
        text = buildFileReport(await fileReport(path));
      } catch (error) {
        notifyError(t("toast.reportFailed"), error);
        return;
      }
      try {
        await navigator.clipboard.writeText(text);
        notify({
          kind: "saved",
          title: t("toast.reportCopied"),
          detail: t("toast.reportCopiedBody"),
        });
      } catch {
        notify({ kind: "error", title: t("toast.reportFailed") });
      }
    },
    [notify, notifyError, t],
  );

  const openPath = useCallback(
    async (path: string, password?: string, gameFolder?: string) => {
      setBusy(true);
      const cover = window.setTimeout(() => setOpening(baseName(path)), OPEN_COVER_DELAY_MS);
      try {
        const info = gameFolder
          ? await openFileWithGameFolder(path, gameFolder, settings.backupFolders)
          : await openFile(path, password, settings.backupFolders);
        setFile(info);
        resetChanges();
        setHistory(NO_HISTORY);
        setInvalid(false);
        setEditorKey((key) => key + 1);
        setPasswordRequest(null);
        notify({
          kind: "opened",
          title: t("bell.opened", { name: info.fileName }),
          detail: info.path,
          toast: false,
        });
      } catch (error) {
        if (
          isApiError(error) &&
          (ErrorCodes.passwordRequired.includes(error.code) ||
            ErrorCodes.wrongPassword.includes(error.code))
        ) {
          setPasswordRequest({
            path,
            wrong: ErrorCodes.wrongPassword.includes(error.code),
            folderTried: gameFolder !== undefined,
          });
        } else {
          setPasswordRequest(null);
          const unknownFormat = isApiError(error) && error.code === ErrorCodes.unknownFormat;
          notifyError(
            t("toast.openFailed"),
            error,
            unknownFormat
              ? { label: t("toast.copyReport"), run: () => void copyFileReport(path) }
              : undefined,
          );
        }
      } finally {
        window.clearTimeout(cover);
        setOpening(null);
        setBusy(false);
      }
    },
    [copyFileReport, notify, notifyError, resetChanges, settings.backupFolders, t],
  );

  /**
   * Lets the player pick the game's install folder; the host then searches its own files for the
   * password and tries the file again. A failure leaves the password dialog open.
   */
  const pickGameFolder = useCallback(async () => {
    if (!passwordRequest) return;
    const folder = await openDialog({
      directory: true,
      multiple: false,
      title: t("password.pickFolderTitle"),
    });
    if (typeof folder !== "string") return;
    await openPath(passwordRequest.path, undefined, folder);
  }, [openPath, passwordRequest, t]);

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

  /**
   * Opens a pasted save code. The paste dialog has already asked about unsaved changes; a refusal
   * goes back to it to show.
   */
  const openPastedCode = useCallback(
    async (text: string): Promise<OpenCodeResult> => {
      setBusy(true);
      try {
        const info = await openCode(text, settings.appFolder, settings.backupFolders);
        setFile(info);
        resetChanges();
        setHistory(NO_HISTORY);
        setInvalid(false);
        setEditorKey((key) => key + 1);
        setPasteRequest(null);
        notify({ kind: "opened", title: t("bell.openedCode"), toast: false });
        return { ok: true };
      } catch (error) {
        return { ok: false, error };
      } finally {
        setBusy(false);
      }
    },
    [notify, resetChanges, settings.appFolder, settings.backupFolders, t],
  );

  /** Opens the paste dialog, unless another action runs or the JSON editor is open. */
  const requestPaste = useCallback(
    (text = "") => {
      if (busy || jsonOpen) return;
      setPasteRequest({ text });
    },
    [busy, jsonOpen],
  );

  /**
   * Copies the open document as a save code (pending edits included; they stay pending). When the
   * clipboard cannot be written, the code is shown to copy by hand.
   */
  const copyAsCode = useCallback(async () => {
    if (!file) return;
    setBusy(true);
    try {
      const code = await copyCode();
      try {
        await navigator.clipboard.writeText(code);
        notify({ kind: "saved", title: t("toast.codeCopied"), detail: t("toast.codeCopiedBody") });
      } catch {
        setCodeText(code);
      }
    } catch (error) {
      notifyError(t("toast.copyCodeFailed"), error);
    } finally {
      setBusy(false);
    }
  }, [file, notify, notifyError, t]);

  const chooseAndOpen = useCallback(async () => {
    if (busy) return;
    try {
      const picked = await openDialog({ multiple: false, directory: false, title: t("dialog.openTitle") });
      if (typeof picked === "string") requestOpen(picked);
    } catch (error) {
      notifyError(t("toast.openFailed"), error);
    }
  }, [busy, notifyError, requestOpen, t]);

  /** Picks the second save of a comparison and opens its read-only dialog. */
  const chooseCompare = useCallback(async () => {
    if (!file || busy) return;
    try {
      const picked = await openDialog({
        multiple: false,
        directory: false,
        title: t("dialog.compareTitle"),
      });
      if (typeof picked === "string") setComparePath(picked);
    } catch (error) {
      notifyError(t("compare.failed"), error);
    }
  }, [busy, file, notifyError, t]);

  /** Closes the comparison and scrolls the editor to the value the player picked. */
  const revealCompareRow = useCallback((path: string) => {
    window.requestAnimationFrame(() => editorRef.current?.reveal(path));
  }, []);

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

  // A file handed over by the start ("Decode with SavePrism") or by a second instance. The host
  // keeps it until the window takes it, so an early second start is not lost.
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | null = null;
    const openPending = () => {
      void takePendingFile()
        .then((path) => {
          if (!disposed && path) openPathRef.current(path);
        })
        .catch(() => undefined);
    };
    openPending();
    void listen("open-path", openPending)
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      if (stop) stop();
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

  /** Closes the open file (its pending edits are dropped) and goes back to the start screen. */
  const closeOpenFile = useCallback(async () => {
    setCloseFileAsked(false);
    setBusy(true);
    try {
      // Nothing is written; the host only lets go of the document.
      await closeFile();
    } catch {
      // The host keeps it until the next file replaces it; the start screen shows either way.
    } finally {
      setBusy(false);
    }
    setFile(null);
    resetChanges();
    setHistory(NO_HISTORY);
    setInvalid(false);
    setJsonDirty(false);
  }, [resetChanges]);

  /** "Close file": with unsaved changes it asks first. Ignored while another action runs. */
  const requestCloseFile = useCallback(() => {
    if (busy || jsonOpen || file === null) return;
    if (changeCount > 0) {
      setCloseFileAsked(true);
      return;
    }
    void closeOpenFile();
  }, [busy, changeCount, closeOpenFile, file, jsonOpen]);

  const afterSave = useCallback(
    async (target: string, overwrite: boolean) => {
      setBusy(true);
      try {
        const result = await save(
          target,
          overwrite,
          settings.keepBackups,
          settings.keepBackupDays,
          settings.backupFolders,
        );
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
            ? {
                ...current,
                path: result.writtenPath,
                fileName: baseName(result.writtenPath),
                // A pasted code saved as a file is that file from now on.
                pasted: current.pasted && result.writtenPath === current.path,
              }
            : current,
        );
        resetChanges();
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
    [
      notify,
      notifyError,
      resetChanges,
      settings.keepBackups,
      settings.keepBackupDays,
      settings.backupFolders,
      t,
    ],
  );

  const saveAsNew = useCallback(async () => {
    if (!file) return;
    try {
      const target = await saveDialog({
        title: t("dialog.saveTitle"),
        defaultPath: saveAsDefaultPath(file),
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
      resetChanges();
      setEditorKey((key) => key + 1);
      notify({ kind: "info", title: t("bell.undone"), toast: false });
    } catch (error) {
      notifyError(t("toast.discardFailed"), error);
    } finally {
      setBusy(false);
    }
  }, [notify, notifyError, resetChanges, t]);

  /**
   * Shows what an undo, a redo or a "Set all" did. The cards show the values it changed in place
   * (scroll position, searches and open groups stay); a step with very many values makes them read
   * everything again.
   */
  const showHistoryResult = useCallback((result: HistoryResult) => {
    if (result.structural) {
      // Read the lists, groups and counts again in place; scroll position and open cards stay.
      editorRef.current?.reload();
    } else if (result.reload) {
      setEditorKey((key) => key + 1);
    } else {
      editorRef.current?.applyHistory(result.fields);
    }
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
        const info = await restoreBackup(
          name,
          settings.keepBackups,
          settings.keepBackupDays,
          settings.backupFolders,
        );
        setFile(info);
        resetChanges();
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
    [
      notify,
      notifyError,
      resetChanges,
      settings.keepBackups,
      settings.keepBackupDays,
      settings.backupFolders,
      t,
    ],
  );

  /** The names a player reads for paths, from the open editor (the path itself without one). */
  const labelPaths = useCallback(
    async (paths: readonly string[]): Promise<ReadonlyMap<string, string>> =>
      (await editorRef.current?.labels(paths)) ?? new Map<string, string>(),
    [],
  );

  const loadChanges = useCallback(async (): Promise<LabeledChange[]> => {
    try {
      const changes = await pendingChanges();
      const labels = await labelPaths(changes.map((change) => change.path));
      return changes.map((change) => ({ ...change, label: labels.get(change.path) ?? change.path }));
    } catch (error) {
      notifyError(t("toast.changesFailed"), error);
      return [];
    }
  }, [labelPaths, notifyError, t]);

  const openJson = useCallback(() => {
    setJsonState({ status: "loading" });
    setJsonOpen(true);
    exportJson()
      .then((text) => setJsonState({ status: "ready", text }))
      .catch((error: unknown) => {
        const { reason, fix } = describeError(error, t);
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
        const { reason, fix } = describeError(error, t);
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
  // Own names of a pasted code have no game to belong to.
  const saveFolder = file !== null && !file.pasted ? gameFolder(file.path) : null;

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
        tourRunning ||
        whatsNewVersion !== null ||
        backupsOpen ||
        comparePath !== null ||
        locationsOpen ||
        jsonOpen ||
        pasteRequest !== null ||
        codeText !== null ||
        passwordRequest !== null ||
        pendingOpen !== null ||
        closeAsked ||
        closeFileAsked ||
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
        } else if (action === "saveAs" || settings.saveDefault === "saveAs" || file.pasted) {
          // A pasted code has no file of the player's to overwrite.
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

  // Ctrl+V on the start screen opens the paste dialog with the pasted text (nothing opens until
  // the player presses Open). Text boxes and dialogs keep their own paste.
  const pasteHandler = useRef<(event: ClipboardEvent) => void>(() => undefined);
  useEffect(() => {
    pasteHandler.current = (event) => {
      if (file !== null || busy || settingsOpen || tourRunning || pasteRequest !== null) return;
      const target = event.target instanceof Element ? event.target : null;
      if (isTextEditing(target) || document.querySelector('[role="dialog"]') !== null) return;
      const text = pastedText(event.clipboardData);
      if (text === null) return;
      event.preventDefault();
      requestPaste(text);
    };
  });
  useEffect(() => {
    const listener = (event: ClipboardEvent) => pasteHandler.current(event);
    window.addEventListener("paste", listener);
    return () => window.removeEventListener("paste", listener);
  }, []);

  // The notes of a new version open once; the first launch stores the version and shows the tour.
  useEffect(() => {
    let cancelled = false;
    getVersion()
      .then((version) => {
        if (cancelled) return;
        const decision = whatsNewDecision(readStored(LAST_SEEN_VERSION_KEY), version);
        if (decision.store !== null) writeStored(LAST_SEEN_VERSION_KEY, decision.store);
        if (decision.show) setWhatsNewVersion(version);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  const finishTour = useCallback(() => {
    writeStored(TOUR_SEEN_KEY, "1");
    setTourRunning(false);
  }, []);

  const showTourAgain = useCallback(() => {
    setSettingsOpen(false);
    if (file === null) {
      setTourRunning(true);
      return;
    }
    // The tour points at the start screen, so a file that is open is closed first (asking when
    // it has unsaved changes).
    if (changeCount > 0) {
      setTourAfterClose(true);
      setCloseFileAsked(true);
      return;
    }
    void closeOpenFile().then(() => setTourRunning(true));
  }, [changeCount, closeOpenFile, file]);

  const closeWhatsNew = useCallback(() => {
    if (whatsNewVersion !== null) writeStored(LAST_SEEN_VERSION_KEY, whatsNewVersion);
    setWhatsNewVersion(null);
  }, [whatsNewVersion]);

  return (
    <TooltipProvider>
      <div className="relative flex h-screen flex-col bg-background text-foreground">
        <BackgroundPattern />
        <Header
          file={file}
          onCloseFile={requestCloseFile}
          onWhereSaves={() => setLocationsOpen(true)}
          onOpenSettings={() => setSettingsOpen(true)}
        />

        {settingsOpen ? (
          <SettingsPage onClose={() => setSettingsOpen(false)} onShowTour={showTourAgain} />
        ) : (
          <>
            {readOnly ? (
          <div
            role="status"
            className="relative z-10 flex h-8 shrink-0 items-center gap-2 border-b border-warning/40 bg-chrome px-3 text-xs font-medium text-warning"
          >
            <TriangleAlert className="size-3.5 shrink-0" />
            {t(
              file?.notice === "renpy-persistent-signed"
                ? "banner.renpyPersistentSigned"
                : file?.notice === "terraria-layout"
                  ? "banner.terrariaLayout"
                  : "banner.readOnly",
            )}
          </div>
        ) : null}

        {file?.notice === "renpy-signed" ||
        file?.notice === "renpy-key-signed" ||
        file?.notice === "renpy-persistent-key-signed" ? (
          <div
            role="status"
            className="relative z-10 flex h-8 shrink-0 items-center gap-2 border-b border-border bg-chrome px-3 text-xs font-medium text-muted-foreground"
          >
            <Info className="size-3.5 shrink-0" />
            {t(
              file?.notice === "renpy-key-signed"
                ? "banner.renpyKeySigned"
                : file?.notice === "renpy-persistent-key-signed"
                  ? "banner.renpyPersistentKeySigned"
                  : "banner.renpySigned",
            )}
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
                fileName={file.fileName}
                formatLabel={
                  engineLabel(file.fileName, file.codec, file.engine) ??
                  t("header.engineUnknown")
                }
                saveFolder={saveFolder}
                onChangesChanged={applyChanges}
                onInvalidChanged={setInvalid}
                onHistoryChanged={setHistory}
                onEditMany={editMany}
              />
            </main>
          ) : (
            <StartScreen
              dragging={dragging}
              onOpenFile={() => void chooseAndOpen()}
              onPasteCode={() => requestPaste()}
              onWhereSaves={() => setLocationsOpen(true)}
            />
          )}
          {opening !== null ? <OpeningOverlay name={opening} /> : null}
        </div>

        {file ? (
          <BottomBar
            changeCount={changeCount}
            changeKinds={changeKinds}
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
            onCompare={() => void chooseCompare()}
            onSaveAs={() => void saveAsNew()}
            onOverwrite={() => void overwriteOriginal()}
            copyCode={copyCodePlace(file)}
            onCopyCode={() => void copyAsCode()}
          />
        ) : null}
          </>
        )}
        {file && backupsOpen ? (
          <BackupsDialog
            fileName={file.fileName}
            changeCount={changeCount}
            busy={busy}
            onClose={() => setBackupsOpen(false)}
            onRestore={restoreFrom}
          />
        ) : null}
        {comparePath ? (
          <CompareDialog
            path={comparePath}
            fileName={baseName(comparePath)}
            onClose={() => setComparePath(null)}
            onReveal={revealCompareRow}
            labels={labelPaths}
          />
        ) : null}
        {locationsOpen ? (
          <SaveLocationsDialog onClose={() => setLocationsOpen(false)} />
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
        {pasteRequest ? (
          <PasteCodeDialog
            initialText={pasteRequest.text}
            changeCount={file ? changeCount : 0}
            onOpen={openPastedCode}
            onClose={() => setPasteRequest(null)}
          />
        ) : null}
        {codeText !== null ? (
          <CodeTextDialog code={codeText} onClose={() => setCodeText(null)} />
        ) : null}
        {passwordRequest ? (
          <PasswordDialog
            name={baseName(passwordRequest.path)}
            wrong={passwordRequest.wrong}
            folderTried={passwordRequest.folderTried}
            onSubmit={(password) => void openPath(passwordRequest.path, password)}
            onPickFolder={() => void pickGameFolder()}
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
        {closeFileAsked ? (
          <ConfirmDiscardDialog
            reason="close"
            count={changeCount}
            onCancel={() => {
              setCloseFileAsked(false);
              setTourAfterClose(false);
            }}
            onDiscard={() => {
              const startTour = tourAfterClose;
              setTourAfterClose(false);
              void closeOpenFile().then(() => {
                if (startTour) setTourRunning(true);
              });
            }}
          />
        ) : null}
        {tourRunning ? <Tour onDone={finishTour} /> : null}
        {whatsNewVersion !== null ? (
          <WhatsNewDialog version={whatsNewVersion} onClose={closeWhatsNew} />
        ) : null}
        <Toasts />
      </div>
    </TooltipProvider>
  );
}

export default App;
