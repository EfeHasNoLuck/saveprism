import { RadioGroup } from "radix-ui";
import { cn } from "@/lib/utils";

const OPTIONS = ["true", "false"] as const;

const SEGMENT =
  "h-full rounded-[3px] px-1.5 font-mono text-[11px] leading-none outline-none focus-visible:ring-2 focus-visible:ring-ring/60";
const SELECTED = `${SEGMENT} bg-surface-highest font-semibold text-foreground`;
const SELECTED_CHANGED = `${SEGMENT} bg-changed-tag font-semibold text-changed-tag-foreground`;
const IDLE = `${SEGMENT} text-muted-foreground hover:text-foreground disabled:pointer-events-none`;

interface Props {
  value: boolean;
  onChange: (value: boolean) => void;
  /** Accessible name of the group. */
  label: string;
  disabled?: boolean;
  /** The value differs from the one in the file. */
  changed?: boolean;
}

/**
 * A boolean as two segments with the words the file stores, `true` and `false`. It is a radio
 * group: Tab reaches the selected segment, the arrow keys switch it.
 */
export function BoolChoice({ value, onChange, label, disabled = false, changed = false }: Props) {
  const current = value ? "true" : "false";
  return (
    <RadioGroup.Root
      aria-label={label}
      value={current}
      onValueChange={(next) => onChange(next === "true")}
      disabled={disabled}
      orientation="horizontal"
      className={cn(
        "inline-flex h-6.5 shrink-0 items-center gap-0.5 justify-self-start rounded-sm border border-input/70 bg-field p-0.5 dark:border-transparent",
        changed && "border-changed dark:border-changed",
        disabled && "cursor-not-allowed",
      )}
    >
      {OPTIONS.map((option) => (
        <RadioGroup.Item
          key={option}
          value={option}
          className={option !== current ? IDLE : changed ? SELECTED_CHANGED : SELECTED}
        >
          {option}
        </RadioGroup.Item>
      ))}
    </RadioGroup.Root>
  );
}
