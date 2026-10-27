import { Sparkles } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { fieldInfo, getChildren } from "@/api/commands";
import { Button } from "@/components/ui/button";
import {
  askAiValueCount,
  buildAskAiText,
  collectAskAiRows,
  MAX_ASK_AI_VALUES,
  type AskAiRow,
  type AskAiSource,
} from "./askAi";
import { useEditorContext } from "./context";

/** The host as the source of the values inside groups. */
const HOST: AskAiSource = {
  children: async (path, offset) => {
    try {
      const page = await getChildren(path, offset, 200);
      return { items: page.items, total: page.total };
    } catch {
      return null;
    }
  },
  field: async (path) => {
    try {
      const field = await fieldInfo(path);
      return { path: field.path, value: field.value, kind: field.kind, min: field.min, max: field.max };
    } catch {
      return null;
    }
  },
};

/**
 * "Ask AI" in a card header: copies the values the card shows (its search applied, on every page)
 * with a short note, to paste into an AI chat. With `groups`, it also reads the values inside them
 * when pressed (bounded like the note). Hidden when the card shows neither values nor groups.
 */
export function AskAiButton({
  list,
  query,
  rows,
  total,
  groups = [],
}: {
  /** The card's title, as the note names it. */
  list: string;
  query: string;
  /** The values the card shows, on every page. */
  rows: AskAiRow[];
  total: number;
  /** Paths of the card's groups, whose values the note lists after `rows`. */
  groups?: readonly string[];
}) {
  const { t } = useTranslation("editor");
  const { fileName, formatLabel, copyForAi } = useEditorContext();
  const [busy, setBusy] = useState(false);
  if (rows.length === 0 && groups.length === 0) return null;

  async function gather(): Promise<{ rows: readonly AskAiRow[]; total: number | null }> {
    if (groups.length === 0) return { rows, total };
    // The card's own values already fill the note: the groups only add to an unknown total.
    if (rows.length > MAX_ASK_AI_VALUES) return { rows, total: null };
    return collectAskAiRows(rows, groups, query, HOST);
  }

  async function copy() {
    setBusy(true);
    try {
      const found = await gather();
      const text = buildAskAiText({
        t,
        fileName,
        format: formatLabel,
        list,
        query,
        rows: found.rows,
        total: found.total,
      });
      await copyForAi(text, askAiValueCount(found.rows));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Button
      variant="ghost"
      size="sm"
      className="text-xs text-foreground-variant"
      title={t("askAi.tip")}
      disabled={busy}
      onClick={() => void copy()}
    >
      <Sparkles />
      {t("askAi.label")}
    </Button>
  );
}
