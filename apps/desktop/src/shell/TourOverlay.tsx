import { useCallback, useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { availableTourSteps, TOUR_STEPS, type TourStep } from "./tour";

const CARD_WIDTH = 320;
const CARD_HEIGHT = 200;
const GAP = 14;
const EDGE = 8;

interface Box {
  top: number;
  left: number;
  width: number;
  height: number;
}

function cardBox(target: Box | null): Box {
  if (target === null) {
    return {
      top: (window.innerHeight - CARD_HEIGHT) / 2,
      left: (window.innerWidth - CARD_WIDTH) / 2,
      width: CARD_WIDTH,
      height: CARD_HEIGHT,
    };
  }
  let top = target.top + target.height + GAP;
  if (top + CARD_HEIGHT > window.innerHeight - EDGE) {
    top = target.top - GAP - CARD_HEIGHT;
  }
  top = Math.max(EDGE, Math.min(top, window.innerHeight - CARD_HEIGHT - EDGE));
  let left = target.left + target.width / 2 - CARD_WIDTH / 2;
  left = Math.max(EDGE, Math.min(left, window.innerWidth - CARD_WIDTH - EDGE));
  return { top, left, width: CARD_WIDTH, height: CARD_HEIGHT };
}

/** The first-launch tour: spotlights real elements of the start screen and shows a small card. */
export function Tour({ onDone }: { onDone: () => void }) {
  const { t } = useTranslation();
  const titleId = useId();
  const [steps, setSteps] = useState<TourStep[] | null>(null);
  const [index, setIndex] = useState(0);
  const [target, setTarget] = useState<Box | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      const found = availableTourSteps(
        TOUR_STEPS,
        (selector) => document.querySelector(selector) !== null,
      );
      if (found.length === 0) onDone();
      else setSteps(found);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [onDone]);

  const step = steps === null ? null : steps[Math.min(index, steps.length - 1)];

  useEffect(() => {
    if (step === null) return;
    const measure = () => {
      const element = document.querySelector(step.selector);
      if (element instanceof HTMLElement) {
        const rect = element.getBoundingClientRect();
        setTarget({
          top: rect.top,
          left: rect.left,
          width: rect.width,
          height: rect.height,
        });
      }
    };
    measure();
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, true);
    return () => {
      window.removeEventListener("resize", measure);
      window.removeEventListener("scroll", measure, true);
    };
  }, [step]);

  useEffect(() => {
    cardRef.current?.focus();
  }, [index, steps]);

  const next = useCallback(() => {
    if (steps === null) return;
    if (index >= steps.length - 1) onDone();
    else setIndex(index + 1);
  }, [index, onDone, steps]);

  const back = useCallback(() => {
    setIndex((current) => Math.max(0, current - 1));
  }, []);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onDone();
      return;
    }
    if (event.key === "ArrowRight") {
      event.preventDefault();
      next();
      return;
    }
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      back();
      return;
    }
    if (event.key !== "Tab") return;
    const card = cardRef.current;
    if (card === null) return;
    const focusable = Array.from(
      card.querySelectorAll<HTMLElement>("button:not([disabled])"),
    );
    if (focusable.length === 0) {
      event.preventDefault();
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || active === card)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  };

  if (steps === null || step === null) return null;

  const box = cardBox(target);
  const last = index >= steps.length - 1;

  return (
    <div className="fixed inset-0 z-40" role="presentation">
      {target !== null ? (
        <div
          className="pointer-events-none fixed rounded-md border-2 border-primary shadow-[0_0_0_9999px_rgba(0,0,0,0.55)] transition-[top,left,width,height] duration-150 ease-out motion-reduce:transition-none"
          style={{
            top: target.top - 4,
            left: target.left - 4,
            width: target.width + 8,
            height: target.height + 8,
          }}
        />
      ) : (
        <div className="fixed inset-0 bg-black/55" />
      )}

      <div
        ref={cardRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="fixed z-50 flex flex-col gap-2 rounded-lg border border-primary/30 bg-popover p-4 text-popover-foreground shadow-xl shadow-black/30 outline-none animate-in fade-in-0 zoom-in-95 duration-150 ease-out motion-reduce:animate-none"
        style={{ top: box.top, left: box.left, width: box.width }}
      >
        <div className="flex items-center justify-between gap-2">
          <span className="text-[0.6875rem] font-semibold tracking-[0.04em] text-muted-foreground uppercase">
            {t("tour.step", { current: index + 1, total: steps.length })}
          </span>
          <button
            type="button"
            onClick={onDone}
            className="rounded-sm px-1.5 py-0.5 text-[0.71875rem] text-muted-foreground outline-none hover:bg-surface-high hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
          >
            {t("tour.skip")}
          </button>
        </div>
        <h2 id={titleId} className="text-[0.9375rem] font-semibold">
          {t(step.titleKey)}
        </h2>
        <p className="text-[0.8125rem] leading-5 text-foreground-variant">{t(step.bodyKey)}</p>
        <div className="mt-1 flex items-center justify-between gap-2">
          <Button variant="ghost" size="sm" onClick={back} disabled={index === 0}>
            {t("tour.back")}
          </Button>
          <Button size="sm" onClick={next}>
            {last ? t("tour.done") : t("tour.next")}
          </Button>
        </div>
      </div>
    </div>
  );
}
