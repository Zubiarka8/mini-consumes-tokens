import type en from "./en";
import { atlasEs } from "./landing/atlas.es";
import { contextEs } from "./landing/context.es";
import { shellEs } from "./landing/shell.es";

const es: Record<keyof typeof en, string> = {
  ...shellEs,
  ...contextEs,
  ...atlasEs,
};
export default es;
