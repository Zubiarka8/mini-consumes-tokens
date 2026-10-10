import { createMDX } from 'fumadocs-mdx/next';

const withMDX = createMDX();

/** @type {import('next').NextConfig} */
const config = {
  output: 'export',
  reactStrictMode: true,
  // Stop `next dev` writing AGENTS.md/CLAUDE.md here; repo agent rules live in rules.md.
  agentRules: false,
  // GitHub Pages serves the site under /<repo>; set by the deploy workflow, empty locally.
  basePath: process.env.NEXT_PUBLIC_BASE_PATH ?? '',
};

export default withMDX(config);
