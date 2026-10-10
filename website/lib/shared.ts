import { createGetUrl } from 'fumadocs-core/source';

export const appName = 'mini-consumes-tokens';
export const docsRoute = '/docs';
export const docsContentRoute = '/llms.mdx/docs';

// Mirrors next.config.mjs basePath; Next only prefixes it for <Link>, not fetch() or plain text.
export const basePath = process.env.NEXT_PUBLIC_BASE_PATH ?? '';

export const gitConfig = {
  user: 'Zubiarka8',
  repo: 'mini-consumes-tokens',
  branch: 'main',
};

const getContentUrl = createGetUrl(docsContentRoute);

export function getPageMarkdownUrl(page: { slugs: string[]; locale?: string }) {
  const segments = [...page.slugs, 'content.md'];

  return { segments, url: basePath + getContentUrl(segments, page.locale) };
}

export function withBasePath(markdown: string) {
  return markdown.replaceAll(`](${docsRoute}`, `](${basePath}${docsRoute}`);
}
