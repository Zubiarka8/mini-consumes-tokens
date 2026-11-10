"use client";
import Image from "next/image";
import Link from "next/link";
import {
  ArrowRight,
  ArrowUpRight,
  Network,
  ScanLine,
  GitBranch,
  Menu,
  X,
  ChevronDown,
} from "lucide-react";
import { MotionConfig, motion, useReducedMotion } from "motion/react";
import { useTranslation } from "react-i18next";
import { useTheme } from "fumadocs-ui/provider/base";
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { basePath } from "@/lib/shared";
import { localeKey } from "./locale";
import { ContextDemo } from "./context-demo";
import { CopyCommand, setupCommands } from "./copy-command";
import { ObsidianShowcase, WebShowcase } from "./showcase";
import { RevealOnScroll } from "./reveal";
import { Constellation } from "./constellation";
const subscribe = () => () => {};
const repository = "https://github.com/Zubiarka8/mini-consumes-tokens";
// Each FAQ answer points at the docs section that explains it in depth.
const faqDocs = [
  "/docs/schema/#overview",
  "/docs/clients/",
  "/docs/limitations/",
  "/docs/managing/#usually-you-do-nothing",
  "/docs/tools/#which-tool-for-which-question",
  "/docs/architecture/#language-crates",
  "/docs/installation/",
  "/docs/exclusion/#your-own-rules-mctignore",
  "/docs/tools/#hybrid_search_like",
  null,
  "/docs/cli/#commands-at-a-glance",
  "/docs/cli/#commands-at-a-glance",
];
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
export function Landing() {
  const { t, i18n } = useTranslation();
  const reduced = useReducedMotion();
  const { theme, setTheme } = useTheme();
  const [menuOpen, setMenuOpen] = useState(false);
  const headerRef = useRef<HTMLElement>(null);
  const menuToggleRef = useRef<HTMLButtonElement>(null);
  const mounted = useSyncExternalStore(
    subscribe,
    () => true,
    () => false,
  );
  useEffect(() => {
    if (!menuOpen) return;
    const desktop = window.matchMedia("(min-width: 1101px)");
    function closeOutside(event: PointerEvent) {
      if (
        event.target instanceof Node &&
        !headerRef.current?.contains(event.target)
      ) {
        setMenuOpen(false);
      }
    }
    function closeOnDesktop(event: MediaQueryListEvent) {
      if (event.matches) setMenuOpen(false);
    }
    document.addEventListener("pointerdown", closeOutside);
    desktop.addEventListener("change", closeOnDesktop);
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      desktop.removeEventListener("change", closeOnDesktop);
    };
  }, [menuOpen]);
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
      <div className="landing v-editorial" id="top">
        <header
          ref={headerRef}
          className="landing-header shell"
          data-menu-ready={mounted || undefined}
          data-menu-open={menuOpen || undefined}
          onKeyDown={(event) => {
            if (event.key === "Escape" && menuOpen) {
              setMenuOpen(false);
              menuToggleRef.current?.focus();
            }
          }}
        >
          <Link
            href="/"
            className="brand"
            aria-label="mini-consumes-tokens"
            onClick={() => setMenuOpen(false)}
          >
            <img
              src={`${basePath}/img/logo.png`}
              alt=""
              width={40}
              height={40}
            />
            <span>mini-consumes-tokens</span>
          </Link>
          <button
            ref={menuToggleRef}
            type="button"
            className="mobile-menu-toggle"
            aria-label={t(menuOpen ? "navClose" : "navOpen")}
            aria-expanded={menuOpen}
            aria-controls="landing-navigation landing-actions"
            onClick={() => setMenuOpen((open) => !open)}
          >
            {menuOpen ? (
              <X size={24} aria-hidden="true" />
            ) : (
              <Menu size={24} aria-hidden="true" />
            )}
          </button>
          <nav
            id="landing-navigation"
            aria-label={t("nav")}
            onClick={() => setMenuOpen(false)}
          >
            <a href="#problem">{t("navWhy")}</a>
            <a href="#capabilities">{t("capabilities")}</a>
            <a href="#web">{t("navWeb")}</a>
            <a href="#obsidian">{t("navObsidian")}</a>
            <a href="#start">{t("navStart")}</a>
            <a href="#faq">{t("navFaq")}</a>
          </nav>
          <div id="landing-actions" className="header-actions">
            <label className="theme-control">
              <span className="sr-only">{t("theme")}</span>
              <select
                aria-label={t("theme")}
                value={mounted ? theme : "system"}
                onChange={(event) => setTheme(event.target.value)}
              >
                <option value="system">{t("themeSystem")}</option>
                <option value="light">{t("themeLight")}</option>
                <option value="dark">{t("themeDark")}</option>
              </select>
              <ChevronDown
                className="header-control-chevron"
                size={16}
                aria-hidden="true"
              />
            </label>
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
              <ChevronDown
                className="header-control-chevron"
                size={16}
                aria-hidden="true"
              />
            </label>
            <DocsLink className="header-docs">{t("docs")}</DocsLink>
          </div>
        </header>
        <RevealOnScroll />
        <main id="main" tabIndex={-1}>
          <Constellation />
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
              <span className="mono">{t("principle1")}</span>
              <ArrowRight size={16} aria-hidden="true" />
              <span className="mono">{t("principle2")}</span>
              <ArrowRight size={16} aria-hidden="true" />
              <span className="mono">{t("principle3")}</span>
              <p>{t("mapNote")}</p>
            </div>
          </div>
          <section
            id="problem"
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
          <WebShowcase />
          <ObsidianShowcase />
          <section
            id="start"
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
                {setupCommands.map(({ command, label }) => (
                  <CopyCommand
                    key={command}
                    command={command}
                    label={t(label)}
                  />
                ))}
                <p className="caption">{t("reconnect")}</p>
              </div>
            </div>
          </section>
          <section
            id="prompting"
            className="prompting shell section start-grid"
            aria-labelledby="prompting-title"
          >
            <div>
              <p className="eyebrow">{t("promptingLabel")}</p>
              <h2 id="prompting-title">{t("promptingTitle")}</h2>
              <p className="body-copy">{t("promptingText")}</p>
            </div>
            <div className="command-panel">
              {[1, 2, 3, 4].map((i) => (
                <CopyCommand
                  key={i}
                  command={t(`prompt${i}`)}
                  label={t(`promptLabel${i}`)}
                />
              ))}
              <p className="caption">{t("promptingNote")}</p>
            </div>
          </section>
          <section
            className="docs-invite shell section"
            aria-labelledby="docs-title"
          >
            <div className="docs-art" aria-hidden="true">
              <Image
                className="docs-art-image"
                src={`${basePath}/img/docs-field-guide.webp`}
                alt=""
                width={1448}
                height={1086}
                unoptimized
              />
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
          <section
            id="faq"
            className="faq shell section"
            aria-labelledby="faq-title"
          >
            <div>
              <p className="eyebrow">{t("faqLabel")}</p>
              <h2 id="faq-title">{t("faqTitle")}</h2>
            </div>
            <div>
              {Array.from({ length: 12 }, (_, i) => i + 1).map((i) => (
                <details key={i} name="faq">
                  <summary>
                    {t(`faq${i}`)}
                    <span aria-hidden="true" />
                  </summary>
                  <p>{t(`answer${i}`)}</p>
                  {faqDocs[i - 1] && (
                    <Link
                      href={faqDocs[i - 1] as string}
                      className="faq-more text-link"
                    >
                      {t("faqMore")}
                      <ArrowRight size={14} aria-hidden="true" />
                    </Link>
                  )}
                </details>
              ))}
            </div>
          </section>
        </main>
        <footer className="landing-footer shell">
          <div>
            <img
              src={`${basePath}/img/logo.png`}
              alt=""
              width={40}
              height={40}
            />
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
