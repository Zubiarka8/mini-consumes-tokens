import type { Metadata } from "next";
import { LandingLocale } from "@/components/landing/locale";
import { Landing } from "@/components/landing/landing";
import "./landing.css";

const title = "mini-consumes-tokens — A code atlas for your AI agent";
const description =
  "Turn source code into a local symbol graph. Give MCP coding assistants focused context with definitions, relationships and impact analysis.";
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
        alt: "mini-consumes-tokens code atlas",
      },
    ],
  },
};
export default function HomePage() {
  return (
    <LandingLocale>
      <Landing />
    </LandingLocale>
  );
}
