import { RadioGroup } from "radix-ui";
import { cn } from "@/lib/utils";

const OPTIONS = ["true", "false"] as const;

const SELECTED =
  "h-full rounded-[5px] bg-card px-2.5 font-mono text-xs font-semibold text-foreground shadow-sm ring-1 ring-border outline-none focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input dark:ring-0";
const IDLE =
  "h-full rounded-[5px] px-2.5 font-mono text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none";

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
        "inline-flex h-7 shrink-0 items-center gap-0.5 rounded-md border bg-muted p-0.5 dark:bg-background",
        changed && "border-changed bg-changed/10 dark:bg-changed/10",
        disabled && "cursor-not-allowed opacity-60",
      )}
    >
      {OPTIONS.map((option) => (
        <RadioGroup.Item key={option} value={option} className={option === current ? SELECTED : IDLE}>
          {option}
        </RadioGroup.Item>
      ))}
    </RadioGroup.Root>
  );
}
