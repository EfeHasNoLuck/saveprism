import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  discardChanges,
  ErrorCodes,
  isApiError,
  openFile,
  pendingChanges,
  save,
  type Change,
  type OpenInfo,
} from "@/api/commands";
import { TooltipProvider } from "@/components/ui/tooltip";
import { EditorView } from "@/editor";
import { BackgroundPattern } from "./shell/BackgroundPattern";
import { BottomBar } from "./shell/BottomBar";
import { ConfirmDiscardDialog } from "./shell/ConfirmDiscardDialog";
import { useNotify, useSettings } from "./shell/context";
import { baseName } from "./shell/engine";
import { describeError } from "./shell/errors";
import { exportJson } from "./shell/exportApi";
import { Header } from "./shell/Header";
import { JsonDialog, type JsonState } from "./shell/JsonDialog";
import { PasswordDialog } from "./shell/PasswordDialog";
import { addRecent, loadRecent, saveRecent, type RecentFile } from "./shell/recent";
import { SettingsDialog } from "./shell/SettingsDialog";
import { browserStorage } from "./shell/settings";
import { StartScreen } from "./shell/StartScreen";
import { Toasts } from "./shell/Toasts";

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
  const [passwordRequest, setPasswordRequest] = useState<PasswordRequest | null>(null);
  const [pendingOpen, setPendingOpen] = useState<string | null>(null);
  const [jsonOpen, setJsonOpen] = useState(false);
  const [jsonState, setJsonState] = useState<JsonState>({ status: "loading" });

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
      try {
        const info = await openFile(path, password);
        setFile(info);
        setChangeCount(0);
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
        setBusy(false);
      }
    },
    [notify, notifyError, rememberRecent, t],
  );

  /** Opens `path`; with unsaved changes it asks first. Ignored while another action runs. */
  const requestOpen = useCallback(
    (path: string) => {
      if (busy) return;
      if (file && changeCount > 0) {
        setPendingOpen(path);
        return;
      }
      void openPath(path);
    },
    [busy, changeCount, file, openPath],
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

  const readOnly = file !== null && !file.editable;

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
            className="relative z-10 flex items-center gap-2 border-b border-warning/40 bg-warning/10 px-4 py-2 text-sm text-warning"
          >
            <TriangleAlert className="size-4 shrink-0" />
            {t("banner.readOnly")}
          </div>
        ) : null}

        {file ? (
          <main className="relative z-10 flex min-h-0 flex-1 flex-col overflow-auto">
            <EditorView
              key={editorKey}
              readOnly={readOnly}
              onChangesChanged={setChangeCount}
              onInvalidChanged={setInvalid}
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

        {file ? (
          <BottomBar
            changeCount={changeCount}
            readOnly={readOnly}
            invalid={invalid}
            busy={busy}
            saveDefault={settings.saveDefault}
            loadChanges={loadChanges}
            onUndoAll={() => void undoAll()}
            onOpenJson={openJson}
            onSaveAs={() => void saveAsNew()}
            onOverwrite={() => void overwriteOriginal()}
          />
        ) : null}

        <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
        {jsonOpen ? (
          <JsonDialog open onOpenChange={setJsonOpen} state={jsonState} />
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
        <Toasts />
      </div>
    </TooltipProvider>
  );
}

export default App;
