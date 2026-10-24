import { Tooltip } from "radix-ui";
import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type Ref,
} from "react";
import { useTranslation } from "react-i18next";
import {
  addInventory as addInventoryCommand,
  addValue as addValueCommand,
  gameView,
  historyState,
  pendingChanges,
  removeValue as removeValueCommand,
  setDataFolder as setDataFolderCommand,
  type ApiError,
  type Change,
  type Field,
  type GameView,
  type HistoryState,
} from "@/api/commands";
import { useNotify } from "@/shell/context";
import { describeError } from "@/shell/errors";
import { EditorContext, toApiError, type EditorContextValue } from "./context";
import { registerEditorBundles } from "./i18n";
import { RpgMaker } from "./RpgMaker";
import { SugarCube } from "./SugarCube";
import { ValuesCard } from "./ValuesCard";

/** What the app can ask of the editor from outside. */
export interface EditorHandle {
  /** Shows what an undo or redo changed: the values it touched, then the counts again. */
  applyHistory: (changed: Field[]) => void;
  /** Reads the view again after a step that added or removed entries (scroll and open cards stay). */
  reload: () => void;
}

export interface EditorViewProps {
  readOnly: boolean;
  onChangesChanged: (changes: Change[]) => void;
  onInvalidChanged: (invalid: boolean) => void;
  /** After every change of the document: whether Undo and Redo are possible. */
  onHistoryChanged: (state: HistoryState) => void;
  /** A list's "Set all": many values to one text as one undo step (see `EditorContextValue`). */
  onEditMany: (paths: string[], text: string) => Promise<boolean>;
  ref?: Ref<EditorHandle>;
}

export function EditorView({
  readOnly,
  onChangesChanged,
  onInvalidChanged,
  onHistoryChanged,
  onEditMany,
  ref,
}: EditorViewProps) {
  registerEditorBundles();
  const { t } = useTranslation("editor");
  const { t: tc } = useTranslation();
  const { notify } = useNotify();
  const [view, setView] = useState<GameView | null>(null);
  const [loadError, setLoadError] = useState<ApiError | null>(null);
  const [fields, setFields] = useState<ReadonlyMap<string, Field>>(new Map());
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());
  const invalidPaths = useRef<Set<string>>(new Set());
  const lastInvalid = useRef(false);

  // Keep the latest callbacks without re-creating the context functions.
  const changedRef = useRef(onChangesChanged);
  const invalidRef = useRef(onInvalidChanged);
  const historyRef = useRef(onHistoryChanged);
  useEffect(() => {
    changedRef.current = onChangesChanged;
    invalidRef.current = onInvalidChanged;
    historyRef.current = onHistoryChanged;
  });

  const refreshCount = useCallback(async () => {
    try {
      const [changes, history] = await Promise.all([pendingChanges(), historyState()]);
      changedRef.current(changes);
      historyRef.current(history);
      setPending(new Set(changes.map((c) => c.path)));
    } catch {
      // The count stays as it was; the next edit refreshes it.
    }
  }, []);

  /** Reads the whole view again; the parent keeps the scroll position and open cards. */
  const reloadView = useCallback(async () => {
    try {
      const fresh = await gameView();
      setView(fresh);
      setFields(new Map());
      setLoadError(null);
      await refreshCount();
    } catch (error) {
      setLoadError(toApiError(error, ""));
    }
  }, [refreshCount]);

  const reportCommandError = useCallback(
    (titleKey: string, error: unknown) => {
      const { reason, fix } = describeError(error);
      notify({
        kind: "error",
        title: tc(titleKey),
        detail: tc("toast.errorDetail", { reason, fix }).trim(),
      });
    },
    [notify, tc],
  );

  /** Runs a command that may change the structure, then reads the view and the counts again. */
  const runStructural = useCallback(
    async (call: () => Promise<{ structural: boolean }>, titleKey: string) => {
      try {
        const result = await call();
        if (result.structural) await reloadView();
        else await refreshCount();
        return true;
      } catch (error) {
        reportCommandError(titleKey, error);
        return false;
      }
    },
    [refreshCount, reloadView, reportCommandError],
  );

  useEffect(() => {
    let cancelled = false;
    gameView().then(
      (v) => {
        if (cancelled) return;
        setView(v);
        void refreshCount();
      },
      (e: unknown) => {
        if (cancelled) return;
        setLoadError(toApiError(e, ""));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [refreshCount]);

  const setField = useCallback((field: Field) => {
    setFields((prev) => new Map(prev).set(field.path, field));
  }, []);

  useImperativeHandle(
    ref,
    () => ({
      applyHistory: (changed) => {
        setFields((prev) => {
          const next = new Map(prev);
          for (const field of changed) next.set(field.path, field);
          return next;
        });
        void refreshCount();
      },
      reload: () => {
        void reloadView();
      },
    }),
    [refreshCount, reloadView],
  );

  const reportInvalid = useCallback((path: string, invalid: boolean) => {
    const set = invalidPaths.current;
    if (invalid) set.add(path);
    else set.delete(path);
    const now = set.size > 0;
    if (now !== lastInvalid.current) {
      lastInvalid.current = now;
      invalidRef.current(now);
    }
  }, []);

  const ctx = useMemo<EditorContextValue>(
    () => ({
      readOnly,
      fields,
      setField,
      reportInvalid,
      afterEdit: refreshCount,
      pending,
      editMany: onEditMany,
      addInventory: (list, id, count) =>
        runStructural(() => addInventoryCommand(list, id, count), "toast.addFailed"),
      addValue: (parent, key, kind, text) =>
        runStructural(() => addValueCommand(parent, key, kind, text), "toast.addFailed"),
      removeValue: (path) =>
        runStructural(() => removeValueCommand(path), "toast.removeFailed"),
      setDataFolder: async (folder) => {
        try {
          const fresh = await setDataFolderCommand(folder);
          setView(fresh);
          setFields(new Map());
          await refreshCount();
          return true;
        } catch (error) {
          reportCommandError("toast.dataFolderFailed", error);
          return false;
        }
      },
    }),
    [
      readOnly,
      fields,
      setField,
      reportInvalid,
      refreshCount,
      pending,
      onEditMany,
      runStructural,
      reportCommandError,
    ],
  );

  if (loadError !== null) {
    return (
      <p role="alert" className="p-4 text-[13px] text-destructive">
        {t("loadFailed", { message: loadError.message, hint: loadError.hint }).trim()}
      </p>
    );
  }
  if (view === null) {
    return <p className="p-4 text-[13px] text-muted-foreground">{t("loading")}</p>;
  }

  return (
    <EditorContext.Provider value={ctx}>
      <Tooltip.Provider delayDuration={200}>
        <div className="mx-auto w-full max-w-[1600px] p-3">
          {view.kind === "rpgmaker" ? (
            <RpgMaker view={view.view} />
          ) : view.kind === "sugarcube" ? (
            <SugarCube view={view.view} />
          ) : (
            <ValuesCard rows={view.rows} addParent={view.addParent} />
          )}
        </div>
      </Tooltip.Provider>
    </EditorContext.Provider>
  );
}
