import { ArrowRight, ChevronDown, ChevronUp } from "lucide-react";
import type { ComponentProps } from "react";

/* Lucide icons (ISC licence). Decorative by default: hidden from assistive
   technology and never focusable. */
type IconProps = Omit<ComponentProps<typeof ArrowRight>, "size"> & {
  size?: number | string;
};

export function ArrowIcon({ size = 16, ...props }: IconProps) {
  return (
    <ArrowRight aria-hidden="true" focusable="false" size={size} {...props} />
  );
}

function decorative(Icon: typeof ChevronDown) {
  return function DecorativeIcon({ size = 16, ...props }: IconProps) {
    return <Icon aria-hidden="true" focusable="false" size={size} {...props} />;
  };
}

const DecorativeChevronDown = decorative(ChevronDown);
const DecorativeChevronUp = decorative(ChevronUp);

export {
  DecorativeChevronDown as ChevronDown,
  DecorativeChevronUp as ChevronUp,
};
