import { clsx, type ClassValue } from "clsx";

/**
 * shadcn/ui's class helper, without tailwind-merge: no component here passes
 * a class that conflicts with its defaults, so plain joining is enough.
 */
export const cn = (...inputs: ClassValue[]) => clsx(inputs);
