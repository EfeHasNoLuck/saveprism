import { Bell as BellIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useNotify } from "./context";
import { PathText } from "./PathText";

function timeText(at: number, language: string): string {
  try {
    return new Date(at).toLocaleTimeString(language, { hour: "2-digit", minute: "2-digit" });
  } catch {
    return "";
  }
}

/** The bell: a badge with the unseen events and a popover with what happened in this session. */
export function Bell() {
  const { t, i18n } = useTranslation();
  const { events, unseen, markSeen, clearEvents } = useNotify();
  const count = events.length;

  return (
    // Opening the bell counts as seeing everything in it; closing it too, for lines that came
    // in while it was open.
    <Popover onOpenChange={markSeen}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="relative"
          aria-label={t("header.notifications")}
        >
          <BellIcon />
          {unseen > 0 ? (
            <span className="absolute -top-0.5 -right-0.5 flex min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] leading-4 font-medium text-primary-foreground">
              {unseen > 9 ? "9+" : unseen}
            </span>
          ) : null}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-96">
        <div className="mb-2 flex items-center justify-between">
          <p className="text-sm font-medium">{t("bell.title")}</p>
          {count > 0 ? (
            <Button variant="ghost" size="xs" onClick={clearEvents}>
              {t("bell.clear")}
            </Button>
          ) : null}
        </div>
        {count === 0 ? (
          <p className="py-4 text-center text-sm text-muted-foreground">{t("bell.empty")}</p>
        ) : (
          <ul className="flex max-h-72 flex-col gap-2 overflow-y-auto">
            {events.map((event) => (
              <li key={event.id} className="rounded-md bg-muted px-3 py-2">
                <div className="flex items-baseline justify-between gap-2">
                  <span
                    className={
                      event.kind === "error"
                        ? "min-w-0 text-sm font-medium break-words text-destructive"
                        : "min-w-0 text-sm font-medium break-words"
                    }
                  >
                    <PathText text={event.title} />
                  </span>
                  <span className="shrink-0 text-xs text-muted-foreground">
                    {timeText(event.at, i18n.language)}
                  </span>
                </div>
                {event.detail ? (
                  <p className="mt-0.5 text-xs break-words text-muted-foreground">
                    <PathText text={event.detail} />
                  </p>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </PopoverContent>
    </Popover>
  );
}
