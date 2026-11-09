import type { Metadata } from "next";
import { LandingLocale } from "@/components/landing/locale";
import { Landing } from "@/components/landing/landing";
import "./landing.css";
import "./editorial.css";
import "@fontsource/instrument-serif/latin-400.css";
import "@fontsource/instrument-serif/latin-400-italic.css";

const title = "mini-consumes-tokens: Code Map MCP Server for AI Assistants";
const description =
  "Free, open-source MCP server and CLI that maps your code locally for AI assistants. Browse it as linked Obsidian notes or in a local dashboard.";
export const metadata: Metadata = {
  title,
  description,
  alternates: {
    canonical: "https://zubiarka8.github.io/mini-consumes-tokens/",
  },
  openGraph: {
    title,
    description,
    url: "https://zubiarka8.github.io/mini-consumes-tokens/",
    type: "website",
    images: [
      {
        url: "https://zubiarka8.github.io/mini-consumes-tokens/code-atlas.svg",
        width: 1200,
        height: 630,
        alt: "mini-consumes-tokens: a map of your code",
      },
    ],
  },
};
const jsonLd = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "mini-consumes-tokens",
  applicationCategory: "DeveloperApplication",
  operatingSystem: "Windows, macOS, Linux",
  description,
  url: "https://zubiarka8.github.io/mini-consumes-tokens/",
  license: "https://www.apache.org/licenses/LICENSE-2.0",
  offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
};
export default function HomePage() {
  return (
    <LandingLocale>
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }}
      />
      <Landing />
    </LandingLocale>
  );
}
