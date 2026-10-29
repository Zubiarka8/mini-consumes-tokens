import { useId, useState, type ReactNode } from "react";
import { AnimatePresence, m } from "motion/react";
import { ChevronRight } from "lucide-react";
import { cn } from "../lib/cn";

/** A collapsed group of secondary details: one click away, never in the way of the decision. */
export function Section({
  title,
  children,
  defaultOpen = false,
}: {
  title: ReactNode;
  children: ReactNode;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const id = useId();
  return (
    <div className="border-t border-line">
      <h3 className="m-0 text-[13px] font-semibold">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={id}
          onClick={() => setOpen(!open)}
          className="flex w-full cursor-pointer items-center gap-1.5 rounded-sm border-0 bg-transparent px-0 py-2 text-left text-fg hover:text-primary-ink focus-visible:outline-2 focus-visible:outline-focus"
        >
          <ChevronRight aria-hidden size={14} className={cn("transition-transform", open && "rotate-90")} />
          {title}
        </button>
      </h3>
      <AnimatePresence initial={false}>
        {open && (
          <m.div
            id={id}
            key="body"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.14, ease: "easeOut" }}
            className="overflow-hidden"
          >
            <div className="pb-3 text-[13px]">{children}</div>
          </m.div>
        )}
      </AnimatePresence>
    </div>
  );
}
