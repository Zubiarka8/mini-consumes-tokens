import defaultMdxComponents from 'fumadocs-ui/mdx';
import { Tab, Tabs } from 'fumadocs-ui/components/tabs';
import type { MDXComponents } from 'mdx/types';

// Fiber augments JSX with Three.js elements. Keep those synthetic intrinsic keys
// out of the inferred documentation defaults; the runtime object is unchanged.
type DocumentComponents = Record<string, MDXComponents[string]>;

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...(defaultMdxComponents as DocumentComponents),
    Tab,
    Tabs,
    ...(components as DocumentComponents | undefined),
  } satisfies MDXComponents;
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
