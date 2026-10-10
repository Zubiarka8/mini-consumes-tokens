"use client";
import Link from "next/link";
import {
  ArrowUpRight,
  ArrowRight,
  Network,
  ScanLine,
  GitBranch,
  Copy,
  Check,
  BookOpen,
} from "lucide-react";
import { MotionConfig, motion, useReducedMotion } from "motion/react";
import { useTranslation } from "react-i18next";
import { useState } from "react";
import { localeKey } from "./locale";
import { ContextDemo } from "./context-demo";
import { Atlas } from "./atlas";
const repository = "https://github.com/Zubiarka8/mini-consumes-tokens";
function DocsLink({
  children,
  className = "atlas-button",
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <Link href="/docs/" className={className}>
      {children}
      <ArrowUpRight size={18} aria-hidden="true" />
    </Link>
  );
}
function CopyCommand({ command, label }: { command: string; label: string }) {
  const { t } = useTranslation();
  const [feedback, setFeedback] = useState<"copied" | "copyError" | null>(null);
  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      setFeedback("copied");
    } catch {
      setFeedback("copyError");
    }
  }
  return (
    <div className="command-row">
      <div>
        <span>{label}</span>
        <code>{command}</code>
      </div>
      <button
        type="button"
        onClick={copy}
        aria-label={`${t("copy")}: ${command}`}
      >
        {feedback === "copied" ? (
          <Check size={18} aria-hidden />
        ) : (
          <Copy size={18} aria-hidden />
        )}
      </button>
      <p role="status">{feedback && t(feedback)}</p>
    </div>
  );
}
export function Landing() {
  const { t, i18n } = useTranslation();
  const reduced = useReducedMotion();
  function changeLanguage(language: string) {
    void i18n.changeLanguage(language);
    try {
      localStorage.setItem(localeKey, language);
    } catch {
      /* Selection still works without storage. */
    }
  }
  return (
    <MotionConfig reducedMotion="user">
      <div className="landing" id="top">
        <a className="skip-link" href="#main">
          {t("skip")}
        </a>
        <header className="landing-header shell">
          <Link href="/" className="brand" aria-label="mini-consumes-tokens">
            <Network size={26} strokeWidth={1.4} aria-hidden />
            <span>mini-consumes-tokens</span>
          </Link>
          <nav aria-label={t("nav")}>
            <a href="#how">{t("how")}</a>
            <a href="#capabilities">{t("capabilities")}</a>
          </nav>
          <div className="header-actions">
            <label className="locale-control">
              <span className="sr-only">{t("language")}</span>
              <select
                aria-label={t("language")}
                value={i18n.language}
                onChange={(event) => changeLanguage(event.target.value)}
              >
                <option value="en" lang="en">
                  English
                </option>
                <option value="es" lang="es">
                  Castellano
                </option>
              </select>
            </label>
            <DocsLink className="header-docs">{t("docs")}</DocsLink>
          </div>
        </header>
        <main id="main" tabIndex={-1}>
          <section className="hero shell" aria-labelledby="hero-title">
            <div className="hero-copy">
              <p className="eyebrow">
                <span className="tiny-cross" aria-hidden>
                  +
                </span>
                {t("eyebrow")}
              </p>
              <h1 id="hero-title">
                {t("headline")}
                <span>{t("headlineAccent")}</span>
              </h1>
              <p className="hero-intro">{t("intro")}</p>
              <div className="hero-actions">
                <DocsLink>{t("readDocs")}</DocsLink>
                <a className="text-link" href={repository}>
                  {t("github")}
                  <ArrowUpRight size={16} aria-hidden />
                </a>
              </div>
              <p className="hero-note">
                <span className="status-dot" />
                {t("heroNote")}
              </p>
              <div className="hero-coordinate mono" aria-hidden="true">
                {t("coordinate")}
                <br />
                {t("localFirst")}
              </div>
            </div>
            <ContextDemo />
          </section>
          <div className="principle-bar">
            <div className="shell">
              <span className="mono">tree-sitter</span>
              <span aria-hidden>→</span>
              <span className="mono">SQLite</span>
              <span aria-hidden>→</span>
              <span className="mono">MCP</span>
              <p>{t("mapNote")}</p>
            </div>
          </div>
          <section
            className="problem shell section"
            aria-labelledby="problem-title"
          >
            <div>
              <p className="eyebrow">{t("problemLabel")}</p>
              <h2 id="problem-title">{t("problemTitle")}</h2>
              <p className="body-copy">{t("problemText")}</p>
            </div>
            <div className="comparison">
              <div className="comparison-row">
                <ScanLine aria-hidden size={28} strokeWidth={1.2} />
                <div>
                  <h3>{t("before")}</h3>
                  <p>{t("beforeText")}</p>
                </div>
              </div>
              <motion.div
                className="comparison-row comparison-after"
                initial={false}
                whileInView={{ borderColor: "#59764a" }}
                viewport={{ once: true }}
                transition={{ duration: reduced ? 0 : 0.5 }}
              >
                <Network aria-hidden size={28} strokeWidth={1.2} />
                <div>
                  <h3>{t("after")}</h3>
                  <p>{t("afterText")}</p>
                </div>
              </motion.div>
              <p className="caption">{t("comparisonNote")}</p>
            </div>
          </section>
          <section
            id="how"
            className="mechanism section"
            aria-labelledby="how-title"
          >
            <div className="shell">
              <p className="eyebrow">{t("howLabel")}</p>
              <h2 id="how-title">{t("howTitle")}</h2>
              <div className="steps">
                {[1, 2, 3].map((step) => (
                  <article key={step}>
                    <div className="step-head">
                      <span className="step-number mono">0{step}</span>
                      <ArrowRight size={24} aria-hidden />
                    </div>
                    <h3>{t(`step${step}Title`)}</h3>
                    <p>{t(`step${step}Text`)}</p>
                  </article>
                ))}
              </div>
              <div className="mechanism-atlas">
                <Atlas />
              </div>
            </div>
          </section>
          <section
            id="capabilities"
            className="capabilities shell section"
            aria-labelledby="cap-title"
          >
            <div className="cap-intro">
              <p className="eyebrow">{t("capabilitiesLabel")}</p>
              <h2 id="cap-title">{t("capabilitiesTitle")}</h2>
              <p className="body-copy">{t("audience")}</p>
              <GitBranch
                className="cap-branch"
                size={100}
                strokeWidth={0.65}
                aria-hidden
              />
            </div>
            <div className="cap-list">
              {["discover", "change", "context"].map((key, i) => (
                <article key={key}>
                  <span className="mono cap-number">/ 0{i + 1}</span>
                  <div>
                    <h3>{t(`${key}Title`)}</h3>
                    <p>{t(`${key}Text`)}</p>
                    <code>
                      {
                        [
                          "get_project_overview · find_symbol",
                          "find_references · impact_analysis",
                          "build_context_pack",
                        ][i]
                      }
                    </code>
                  </div>
                </article>
              ))}
              <p className="caption">{t("extras")}</p>
            </div>
          </section>
          <section
            className="getting-started section"
            aria-labelledby="start-title"
          >
            <div className="shell start-grid">
              <div>
                <p className="eyebrow">{t("startLabel")}</p>
                <h2 id="start-title">{t("startTitle")}</h2>
                <p className="body-copy">{t("startText")}</p>
                <Link href="/docs/installation/" className="text-link">
                  {t("guide1")}
                  <ArrowUpRight size={17} aria-hidden />
                </Link>
              </div>
              <div className="command-panel">
                <CopyCommand
                  command="mct-cli --root . init"
                  label={t("initLabel")}
                />
                <CopyCommand
                  command="mct-cli --root . mcp-register"
                  label={t("registerLabel")}
                />
                <CopyCommand
                  command="mct-cli --root . status"
                  label={t("statusLabel")}
                />
                <p className="caption">{t("reconnect")}</p>
              </div>
            </div>
          </section>
          <section
            className="docs-invite shell section"
            aria-labelledby="docs-title"
          >
            <div className="docs-art" aria-hidden="true">
              <BookOpen size={76} strokeWidth={0.8} />
              <span className="mono">
                MCT
                <br />
                {t("fieldGuide")}
                <br />↗
              </span>
            </div>
            <div>
              <p className="eyebrow">{t("docsLabel")}</p>
              <h2 id="docs-title">{t("docsTitle")}</h2>
              <p className="body-copy">{t("docsText")}</p>
              <DocsLink>{t("readDocs")}</DocsLink>
              <div className="guide-links">
                <Link href="/docs/installation/">{t("guide1")}</Link>
                <Link href="/docs/querying/">{t("guide2")}</Link>
                <Link href="/docs/architecture/">{t("guide3")}</Link>
              </div>
            </div>
          </section>
          <section className="faq shell section" aria-labelledby="faq-title">
            <div>
              <p className="eyebrow">{t("faqLabel")}</p>
              <h2 id="faq-title">{t("faqTitle")}</h2>
            </div>
            <div>
              {[1, 2, 3, 4].map((i) => (
                <details key={i}>
                  <summary>
                    {t(`faq${i}`)}
                    <span aria-hidden="true">+</span>
                  </summary>
                  <p>{t(`answer${i}`)}</p>
                </details>
              ))}
            </div>
          </section>
        </main>
        <footer className="landing-footer shell">
          <div>
            <Network size={24} aria-hidden />
            <p>{t("footer")}</p>
          </div>
          <div>
            <a href={repository}>GitHub</a>
            <a href={`${repository}/blob/main/LICENSE`}>{t("license")}</a>
            <a href="#top">{t("top")} ↑</a>
          </div>
        </footer>
      </div>
    </MotionConfig>
  );
}
