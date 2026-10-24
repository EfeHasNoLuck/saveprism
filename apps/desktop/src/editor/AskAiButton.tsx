import { Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { askAiValueCount, buildAskAiText, type AskAiRow } from "./askAi";
import { useEditorContext } from "./context";

/**
 * "Ask AI" in a card header: copies the values the card shows (its search applied, on every page)
 * with a short note, to paste into an AI chat. Hidden when the card shows no values.
 */
export function AskAiButton({
  list,
  query,
  rows,
  total,
}: {
  /** The card's title, as the note names it. */
  list: string;
  query: string;
  /** The values the card shows, on every page. */
  rows: AskAiRow[];
  total: number;
}) {
  const { t } = useTranslation("editor");
  const { fileName, formatLabel, copyForAi } = useEditorContext();
  if (rows.length === 0) return null;

  function copy() {
    const text = buildAskAiText({ t, fileName, format: formatLabel, list, query, rows, total });
    void copyForAi(text, askAiValueCount(rows));
  }

  return (
    <Button
      variant="ghost"
      size="sm"
      className="text-xs text-foreground-variant"
      title={t("askAi.tip")}
      onClick={copy}
    >
      <Sparkles />
      {t("askAi.label")}
    </Button>
  );
}
