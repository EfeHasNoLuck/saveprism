import {
  useDeferredValue,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { Check, ChevronDown, ChevronUp, Copy, FileDown, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { json } from "@codemirror/lang-json";
import { bracketMatching, HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { linter, type Diagnostic } from "@codemirror/lint";
import { highlightSelectionMatches, SearchCursor } from "@codemirror/search";
import { EditorState, StateField, type Extension, type Text } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { describeError } from "./errors";
import { findJsonProblem, type JsonProblem, type JsonProblemKind } from "./jsonCheck";

/** Every load is a new object, so reading the same text again still resets the editor. */
export type JsonState =
  | { status: "loading" }
  | { status: "ready"; text: string }
  | { status: "error"; message: string };

export type ApplyResult = { ok: true } | { ok: false; error: unknown };

const MAX_MATCHES = 10_000;
/** Pause after typing before the syntax check runs (it reads the whole text). */
const LINT_DELAY_MS = 500;

interface Match {
  from: number;
  to: number;
}

/** A syntax problem with its place, tied to the text it was found in. */
interface SyntaxProblem extends JsonProblem {
  line: number;
  column: number;
  /** The loaded text the edits started from. */
  base: Text | null;
}

/** Lower case that keeps the length (the search cursor needs that); other text stays as it is. */
function sameLengthLower(text: string): string {
  const lower = text.toLowerCase();
  return lower.length === text.length ? lower : text;
}

function findMatches(doc: Text, query: string): Match[] {
  const needle = query.trim();
  if (needle === "") return [];
  const cursor = new SearchCursor(doc, needle, 0, doc.length, sameLengthLower);
  const found: Match[] = [];
  while (found.length < MAX_MATCHES && !cursor.next().done) {
    found.push({ from: cursor.value.from, to: cursor.value.to });
  }
  return found;
}

/** Selects a search match and scrolls it into the middle of the view. */
function selectMatch(view: EditorView | null, match: Match | undefined) {
  if (view === null || match === undefined) return;
  view.dispatch({
    selection: { anchor: match.from, head: match.to },
    effects: EditorView.scrollIntoView(match.from, { y: "center" }),
  });
}

function problemAt(doc: Text, problem: JsonProblem, base: Text | null): SyntaxProblem {
  const line = doc.lineAt(problem.offset);
  return { ...problem, line: line.number, column: problem.offset - line.from + 1, base };
}

/** The text as it was loaded into the editor; edits are compared with it. */
const loadedText = StateField.define<Text>({
  create: (state) => state.doc,
  update: (value) => value,
});

/** Syntax colours from the app's own tokens, so both themes follow. */
const jsonHighlight = HighlightStyle.define([
  { tag: tags.propertyName, color: "var(--primary)" },
  { tag: tags.string, color: "var(--success)" },
  { tag: [tags.bool, tags.null], color: "var(--warning)" },
  { tag: [tags.brace, tags.squareBracket, tags.separator, tags.punctuation], color: "var(--muted-foreground)" },
]);

const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    fontSize: "12px",
    backgroundColor: "var(--background)",
    color: "var(--foreground)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": {
    fontFamily: "var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace)",
    lineHeight: "20px",
  },
  ".cm-content": { caretColor: "var(--foreground)" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--foreground)" },
  ".cm-gutters": {
    backgroundColor: "var(--muted)",
    color: "var(--muted-foreground)",
    borderRight: "1px solid var(--border)",
  },
  ".cm-activeLine": { backgroundColor: "color-mix(in oklch, var(--muted) 70%, transparent)" },
  ".cm-activeLineGutter": { backgroundColor: "var(--accent)", color: "var(--foreground)" },
  ".cm-selectionBackground": {
    backgroundColor: "color-mix(in oklch, var(--brand) 25%, transparent) !important",
  },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
    backgroundColor: "color-mix(in oklch, var(--brand) 35%, transparent) !important",
  },
  ".cm-selectionMatch": { backgroundColor: "color-mix(in oklch, var(--brand) 15%, transparent)" },
  "&.cm-focused .cm-matchingBracket": {
    backgroundColor: "color-mix(in oklch, var(--brand) 20%, transparent)",
  },
  ".cm-lintRange-error": {
    backgroundImage: "none",
    textDecoration: "underline wavy var(--destructive)",
    textUnderlineOffset: "3px",
  },
  ".cm-tooltip": {
    backgroundColor: "var(--popover)",
    color: "var(--popover-foreground)",
    border: "1px solid var(--border)",
    borderRadius: "6px",
  },
  ".cm-diagnostic-error": { borderLeftColor: "var(--destructive)" },
});

interface EditorHooks {
  readOnly: boolean;
  label: string;
  /** The text changed (typing, paste, undo); `base` is the loaded text. */
  onDoc: (base: Text, doc: Text) => void;
  /** Result of the delayed syntax check. */
  onProblem: (problem: SyntaxProblem | null) => void;
  message: (kind: JsonProblemKind) => string;
  /** Ctrl+F goes to the search box above the editor. */
  focusSearch: () => boolean;
}

function editorExtensions(hooks: EditorHooks): Extension[] {
  const lint = (view: EditorView): Diagnostic[] => {
    const doc = view.state.doc;
    const base = view.state.field(loadedText);
    if (doc === base) {
      hooks.onProblem(null);
      return [];
    }
    const found = findJsonProblem(doc.toString());
    if (found === null) {
      hooks.onProblem(null);
      return [];
    }
    hooks.onProblem(problemAt(doc, found, base));
    const to = Math.min(found.offset + 1, doc.length);
    return [{ from: found.offset, to, severity: "error", message: hooks.message(found.kind) }];
  };
  return [
    loadedText,
    lineNumbers(),
    highlightActiveLineGutter(),
    history(),
    drawSelection(),
    highlightActiveLine(),
    bracketMatching(),
    highlightSelectionMatches(),
    json(),
    syntaxHighlighting(jsonHighlight),
    keymap.of([{ key: "Mod-f", run: hooks.focusSearch }, ...historyKeymap, ...defaultKeymap]),
    linter(lint, { delay: LINT_DELAY_MS }),
    EditorState.readOnly.of(hooks.readOnly),
    EditorView.contentAttributes.of({ "aria-label": hooks.label }),
    EditorView.updateListener.of((update) => {
      if (update.docChanged) hooks.onDoc(update.state.field(loadedText), update.state.doc);
    }),
    editorTheme,
  ];
}

interface JsonDialogProps {
  state: JsonState;
  /** The open file cannot be written, so the text cannot be applied either. */
  readOnly: boolean;
  /** Closes the editor; edits that are not applied are asked about first. */
  onClose: () => void;
  /** Saves the applied document as a `.json` file (the caller asks for the place and reports). */
  onExport: () => void;
  /** An export is running. */
  exporting: boolean;
  /** Applies the edited text; the caller refreshes `state` and the cards after a success. */
  onApply: (text: string) => Promise<ApplyResult>;
  /** The editor holds edits that are not applied (for the window's close guard). */
  onDirtyChange: (dirty: boolean) => void;
}

/**
 * The open document as JSON text that can be edited: change values, then Apply. Search, Copy and
 * Export .json sit above the text; syntax problems show with their line and column before Apply,
 * and the host's refusal (nothing applied) shows below the text.
 */
export function JsonDialog({
  state,
  readOnly,
  onClose,
  onExport,
  exporting,
  onApply,
  onDirtyChange,
}: JsonDialogProps) {
  const { t } = useTranslation();
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const searchId = useId();

  const [edited, setEdited] = useState<{ base: Text; doc: Text } | null>(null);
  const [syntax, setSyntax] = useState<SyntaxProblem | null>(null);
  const [refused, setRefused] = useState<{ doc: Text; error: unknown } | null>(null);
  const [applying, setApplying] = useState(false);
  const [asking, setAsking] = useState(false);
  const [copied, setCopied] = useState(false);
  const [query, setQuery] = useState("");
  const [current, setCurrent] = useState(0);

  const extensions = useMemo(
    () =>
      editorExtensions({
        readOnly,
        label: t("json.editorLabel"),
        onDoc: (base, doc) => setEdited({ base, doc }),
        onProblem: setSyntax,
        message: (kind) => t(`json.problem.${kind}`),
        focusSearch: () => {
          const input = document.getElementById(searchId);
          if (!(input instanceof HTMLInputElement)) return false;
          input.focus();
          input.select();
          return true;
        },
      }),
    [readOnly, searchId, t],
  );

  const ready = state.status === "ready" ? state : null;
  const editorState = useMemo(
    () => (ready === null ? null : EditorState.create({ doc: ready.text, extensions })),
    [ready, extensions],
  );
  const base = editorState?.doc ?? null;
  const doc = edited !== null && edited.base === base ? edited.doc : base;
  const dirty = useMemo(
    () => doc !== null && base !== null && doc !== base && !doc.eq(base),
    [doc, base],
  );

  // Put each loaded text into the editor; after Apply the cursor stays where it was.
  useEffect(() => {
    const host = hostRef.current;
    if (editorState === null || host === null) return;
    let view = viewRef.current;
    if (view !== null && view.dom.parentElement !== host) {
      view.destroy();
      view = null;
    }
    if (view === null) {
      viewRef.current = new EditorView({ state: editorState, parent: host });
      return;
    }
    const head = Math.min(view.state.selection.main.head, editorState.doc.length);
    view.setState(editorState);
    view.dispatch({
      selection: { anchor: head },
      effects: EditorView.scrollIntoView(head, { y: "center" }),
    });
  }, [editorState]);

  useEffect(
    () => () => {
      viewRef.current?.destroy();
      viewRef.current = null;
    },
    [],
  );

  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);
  useEffect(() => () => onDirtyChange(false), [onDirtyChange]);

  const goTo = (offset: number) => {
    const view = viewRef.current;
    if (view === null) return;
    const at = Math.min(offset, view.state.doc.length);
    view.dispatch({
      selection: { anchor: at },
      effects: EditorView.scrollIntoView(at, { y: "center" }),
    });
    view.focus();
  };

  const searchedDoc = useDeferredValue(doc);
  const matches = useMemo(
    () => (searchedDoc === null ? [] : findMatches(searchedDoc, query)),
    [searchedDoc, query],
  );

  // A new search shows its first match right away (Enter then goes to the second one). Edits to
  // the text do not move the selection.
  const searchedFor = useRef("");
  useEffect(() => {
    if (searchedFor.current === query) return;
    searchedFor.current = query;
    selectMatch(viewRef.current, matches[0]);
  }, [matches, query]);

  const jumpTo = (matchIndex: number) => {
    if (matches.length === 0) return;
    const wrapped = (matchIndex + matches.length) % matches.length;
    setCurrent(wrapped);
    selectMatch(viewRef.current, matches[wrapped]);
  };

  const copy = async () => {
    const view = viewRef.current;
    if (view === null) return;
    try {
      await navigator.clipboard.writeText(view.state.doc.toString());
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      setCopied(false);
    }
  };

  /** Checks the syntax, then hands the text to the host. True when it was applied. */
  const apply = async (): Promise<boolean> => {
    const view = viewRef.current;
    if (view === null || readOnly || applying) return false;
    const now = view.state.doc;
    const textNow = now.toString();
    const found = findJsonProblem(textNow);
    if (found !== null) {
      setSyntax(problemAt(now, found, view.state.field(loadedText)));
      goTo(found.offset);
      return false;
    }
    setApplying(true);
    setRefused(null);
    const result = await onApply(textNow);
    setApplying(false);
    if (!result.ok) {
      setRefused({ doc: now, error: result.error });
      return false;
    }
    return true;
  };

  const requestClose = () => {
    if (dirty && !readOnly) setAsking(true);
    else onClose();
  };

  const applyAndClose = async () => {
    setAsking(false);
    if (await apply()) onClose();
  };

  const shownSyntax = dirty && syntax !== null && syntax.base === base ? syntax : null;
  const shownRefusal = refused !== null && refused.doc === doc ? refused : null;
  let statusLine: ReactNode = null;
  if (shownSyntax !== null) {
    statusLine = (
      <button
        type="button"
        onClick={() => goTo(shownSyntax.offset)}
        className="text-left text-destructive underline-offset-2 outline-none hover:underline focus-visible:underline"
      >
        {t("json.syntaxAt", {
          line: shownSyntax.line,
          column: shownSyntax.column,
          problem: t(`json.problem.${shownSyntax.kind}`),
        })}
      </button>
    );
  } else if (shownRefusal !== null) {
    const { reason, fix } = describeError(shownRefusal.error);
    statusLine = (
      <span role="alert" className="text-destructive">
        {`${t("json.notApplied")} ${reason} ${fix}`.trim()}
      </span>
    );
  } else if (dirty) {
    statusLine = <span className="text-changed-foreground">{t("json.dirty")}</span>;
  }

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : requestClose())}>
      <DialogContent
        className="h-[calc(100vh-2rem)] max-h-[calc(100vh-2rem)] max-w-[calc(100vw-2rem)]"
        onInteractOutside={(event) => {
          // A click on a toast (for example "Show in folder") is not a click outside.
          if (event.target instanceof Element && event.target.closest("[data-toasts]")) {
            event.preventDefault();
          }
        }}
      >
        <DialogHeader>
          <DialogTitle>{t("json.title")}</DialogTitle>
          <DialogDescription>
            {readOnly ? t("json.readOnlyNote") : t("json.editNote")}
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute top-2.5 left-2.5 size-4 text-muted-foreground" />
            <input
              id={searchId}
              type="search"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setCurrent(0);
              }}
              onKeyDown={(event) => {
                if (event.key !== "Enter") return;
                event.preventDefault();
                jumpTo(event.shiftKey ? current - 1 : current + 1);
              }}
              placeholder={t("json.search")}
              aria-label={t("json.search")}
              className="h-9 w-full rounded-lg border border-input bg-background pr-3 pl-8 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
            />
          </div>
          {query.trim() !== "" ? (
            <span className="text-sm whitespace-nowrap text-muted-foreground tabular-nums">
              {matches.length === 0
                ? t("json.noMatches")
                : `${Math.min(current, matches.length - 1) + 1} / ${matches.length}${
                    matches.length >= MAX_MATCHES ? "+" : ""
                  }`}
            </span>
          ) : null}
          <Button
            variant="ghost"
            size="icon"
            aria-label={t("json.previous")}
            disabled={matches.length === 0}
            onClick={() => jumpTo(current - 1)}
          >
            <ChevronUp />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            aria-label={t("json.next")}
            disabled={matches.length === 0}
            onClick={() => jumpTo(current + 1)}
          >
            <ChevronDown />
          </Button>
          <Button variant="outline" disabled={state.status !== "ready"} onClick={() => void copy()}>
            <Copy />
            {copied ? t("common.copied") : t("common.copy")}
          </Button>
          <Button
            variant="outline"
            disabled={state.status !== "ready" || exporting || dirty}
            title={dirty ? t("json.exportApplyFirst") : undefined}
            onClick={onExport}
          >
            <FileDown />
            {t("json.exportJson")}
          </Button>
          {readOnly ? null : (
            <Button disabled={!dirty || applying} onClick={() => void apply()}>
              <Check />
              {applying ? t("json.applying") : t("json.apply")}
            </Button>
          )}
        </div>

        <div className="min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-background">
          {state.status === "loading" ? (
            <p className="p-4 text-sm text-muted-foreground">{t("common.loading")}</p>
          ) : state.status === "error" ? (
            <p role="alert" className="p-4 text-sm text-destructive">
              {state.message}
            </p>
          ) : (
            <div ref={hostRef} className="h-full" />
          )}
        </div>

        <div className="-mt-1 min-h-5 text-sm" aria-live="polite">
          {statusLine}
        </div>

        {asking ? (
          <Dialog open onOpenChange={(open) => (open ? undefined : setAsking(false))}>
            <DialogContent className="max-w-md">
              <DialogHeader>
                <DialogTitle>{t("json.confirmTitle")}</DialogTitle>
                <DialogDescription>{t("json.confirmBody")}</DialogDescription>
              </DialogHeader>
              <DialogFooter>
                <Button variant="ghost" onClick={() => setAsking(false)}>
                  {t("json.keepEditing")}
                </Button>
                <Button
                  variant="destructive"
                  onClick={() => {
                    setAsking(false);
                    onClose();
                  }}
                >
                  {t("json.discard")}
                </Button>
                <Button disabled={applying} onClick={() => void applyAndClose()}>
                  {t("json.apply")}
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
