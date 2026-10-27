import { Check } from "lucide-react";
import { cn } from "@/lib/utils";

export interface StepperStep {
  title: string;
  description?: string;
}

/**
 * A row of numbered steps. `current` is the 0-based index of the step the player is on;
 * steps before it count as done unless `done` says otherwise. `current` of -1 shows a plain
 * reference list with no active step (used by the chatbot card).
 */
export function Stepper({
  steps,
  current,
  done,
  label,
}: {
  steps: StepperStep[];
  current: number;
  done?: boolean[];
  label?: string;
}) {
  return (
    <ol className="mt-3.5 flex list-none" aria-label={label}>
      {steps.map((step, index) => {
        const isDone = done?.[index] ?? (current >= 0 && index < current);
        const isCurrent = index === current;
        return (
          <li
            key={step.title}
            aria-current={isCurrent ? "step" : undefined}
            className="relative flex flex-1 flex-col items-start pr-3 last:pr-0"
          >
            {index < steps.length - 1 ? (
              <span
                aria-hidden="true"
                className={cn(
                  "absolute top-[11px] left-6 -right-3 h-0.5 transition-colors duration-200 ease-out motion-reduce:transition-none",
                  isDone ? "bg-primary" : "bg-surface-highest",
                )}
              />
            ) : null}
            <span
              className={cn(
                "relative z-10 flex size-6 shrink-0 items-center justify-center rounded-full text-xs font-semibold transition-colors duration-200 ease-out motion-reduce:transition-none",
                isDone
                  ? "bg-primary text-primary-foreground"
                  : isCurrent
                    ? "bg-card text-primary ring-2 ring-primary ring-inset"
                    : "bg-surface-highest text-foreground-variant",
              )}
            >
              {isDone ? <Check className="size-3.5" strokeWidth={3} /> : index + 1}
            </span>
            <b
              className={cn(
                "mt-1.5 text-[12.5px] leading-[17px] font-semibold transition-colors duration-200 ease-out motion-reduce:transition-none",
                isCurrent ? "text-primary" : "text-foreground",
              )}
            >
              {step.title}
            </b>
            {step.description ? (
              <span className="mt-px text-[11.5px] leading-[15px] text-muted-foreground">
                {step.description}
              </span>
            ) : null}
          </li>
        );
      })}
    </ol>
  );
}
