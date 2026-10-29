// shadcn/ui's Badge, with the dashboard's tones.
import { cva, type VariantProps } from "class-variance-authority";
import type { HTMLAttributes } from "react";
import { cn } from "../lib/cn";

const badgeVariants = cva(
  "inline-flex max-w-full items-center gap-1 truncate rounded-full border px-2 py-px text-[11.5px] font-medium leading-[18px]",
  {
    variants: {
      tone: {
        neutral: "border-line bg-elevated text-fg-2",
        info: "border-transparent bg-info-soft text-info",
        success: "border-transparent bg-success-soft text-success",
        warning: "border-transparent bg-warning-soft text-warning",
        danger: "border-transparent bg-danger-soft text-danger",
      },
    },
    defaultVariants: { tone: "neutral" },
  },
);

export type Tone = NonNullable<VariantProps<typeof badgeVariants>["tone"]>;

export function Badge({
  className,
  tone,
  ...props
}: HTMLAttributes<HTMLSpanElement> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ tone }), className)} {...props} />;
}

/** Tone of each review state, as components.js's STATES. */
const STATE_TONES: Record<string, Tone> = {
  unreviewed: "warning",
  "in-review": "info",
  confirmed: "danger",
  "false-positive": "info",
  resolved: "success",
  dismissed: "neutral",
  "needs-reevaluation": "warning",
  "resolved-but-detected": "info",
};
export const stateTone = (state: string): Tone => STATE_TONES[state] ?? "neutral";
