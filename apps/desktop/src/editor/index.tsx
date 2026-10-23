import { Tooltip } from "radix-ui";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  gameView,
  pendingChanges,
  type ApiError,
  type Field,
  type GameView,
} from "@/api/commands";
import { EditorContext, toApiError, type EditorContextValue } from "./context";
import { registerEditorBundles } from "./i18n";
import { RpgMaker } from "./RpgMaker";
import { ValuesCard } from "./ValuesCard";

export interface EditorViewProps {
  readOnly: boolean;
  onChangesChanged: (count: number) => void;
  onInvalidChanged: (invalid: boolean) => void;
}

export function EditorView({ readOnly, onChangesChanged, onInvalidChanged }: EditorViewProps) {
  registerEditorBundles();
  const { t } = useTranslation("editor");
  const [view, setView] = useState<GameView | null>(null);
  const [loadError, setLoadError] = useState<ApiError | null>(null);
  const [fields, setFields] = useState<ReadonlyMap<string, Field>>(new Map());
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());
  const invalidPaths = useRef<Set<string>>(new Set());
  const lastInvalid = useRef(false);

  // Keep the latest callbacks without re-creating the context functions.
  const changedRef = useRef(onChangesChanged);
  const invalidRef = useRef(onInvalidChanged);
  useEffect(() => {
    changedRef.current = onChangesChanged;
    invalidRef.current = onInvalidChanged;
  });

  const refreshCount = useCallback(async () => {
    try {
      const changes = await pendingChanges();
      changedRef.current(changes.length);
      setPending(new Set(changes.map((c) => c.path)));
    } catch {
      // The count stays as it was; the next edit refreshes it.
    }
  }, []);

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
    () => ({ readOnly, fields, setField, reportInvalid, afterEdit: refreshCount, pending }),
    [readOnly, fields, setField, reportInvalid, refreshCount, pending],
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
          {view.kind === "rpgmaker" ? <RpgMaker view={view.view} /> : <ValuesCard rows={view.rows} />}
        </div>
      </Tooltip.Provider>
    </EditorContext.Provider>
  );
}
