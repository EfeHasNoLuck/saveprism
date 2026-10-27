/** The start screen's questions. The text lives in the locale files. */

export type FaqIcon = "saves" | "shield" | "games" | "help" | "ai" | "undo";

export interface FaqItem {
  id: string;
  icon: FaqIcon;
  /** Question key in `common.json` (`start.faq.<id>.question`). */
  questionKey: string;
  /** Answer key in `common.json` (`start.faq.<id>.answer`). */
  answerKey: string;
}

export const FAQ_ITEMS: readonly FaqItem[] = [
  { id: "saves", icon: "saves", questionKey: "start.faq.saves.question", answerKey: "start.faq.saves.answer" },
  {
    id: "original",
    icon: "shield",
    questionKey: "start.faq.original.question",
    answerKey: "start.faq.original.answer",
  },
  { id: "games", icon: "games", questionKey: "start.faq.games.question", answerKey: "start.faq.games.answer" },
  {
    id: "notOpen",
    icon: "help",
    questionKey: "start.faq.notOpen.question",
    answerKey: "start.faq.notOpen.answer",
  },
  { id: "ai", icon: "ai", questionKey: "start.faq.ai.question", answerKey: "start.faq.ai.answer" },
  { id: "undo", icon: "undo", questionKey: "start.faq.undo.question", answerKey: "start.faq.undo.answer" },
] as const;
