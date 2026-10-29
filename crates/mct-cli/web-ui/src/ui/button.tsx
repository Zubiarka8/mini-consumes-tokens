// shadcn/ui's Button, without Radix's Slot (no `asChild` is needed here).
import { cva, type VariantProps } from "class-variance-authority";
import type { ButtonHTMLAttributes } from "react";
import { cn } from "../lib/cn";

const buttonVariants = cva(
  "inline-flex shrink-0 cursor-pointer items-center justify-center gap-1.5 whitespace-nowrap rounded-ui border font-sans font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus disabled:cursor-not-allowed disabled:opacity-55 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default: "border-control bg-surface text-fg hover:bg-hover",
        primary: "border-primary bg-primary text-on-primary hover:bg-primary-hover",
        ghost: "border-transparent bg-transparent text-fg hover:bg-hover",
      },
      size: {
        default: "h-8 px-3 text-[13px]",
        sm: "h-7 px-2 text-xs",
        icon: "size-7 p-0",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  },
);

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & VariantProps<typeof buttonVariants>;

export function Button({ className, variant, size, type = "button", ...props }: ButtonProps) {
  return <button type={type} className={cn(buttonVariants({ variant, size }), className)} {...props} />;
}
