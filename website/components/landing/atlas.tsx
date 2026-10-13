"use client";
import {
  Component,
  Fragment,
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { motion, useInView } from "motion/react";
import { useTranslation } from "react-i18next";
import Link from "next/link";
import { ArrowUpRight, Maximize2, Minimize2 } from "lucide-react";

import {
  examples,
  layerKeys,
  tourStages,
  type AtlasLayers,
  type Cue,
} from "./atlas-data";
import { fileSymbols } from "./atlas-symbols";
import { ArrowIcon } from "./arrow-icon";
import { CopyCommand, setupCommands } from "./copy-command";

// Plain-language steps shown above the scene, in the order the data travels.
const flowKeys = ["flowFiles", "flowMap", "flowNeeded"] as const;
const SpatialAtlas = lazy(() => import("./spatial-atlas"));
class SceneBoundary extends Component<
  { children: ReactNode; onFailure: () => void },
  { failed: boolean }
> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch() {
    this.props.onFailure();
  }
  render() {
    return this.state.failed ? null : this.props.children;
  }
}
export function Atlas() {
  const { t } = useTranslation();
  const [selected, setSelected] = useState(0);
  const [expanded, setExpanded] = useState(false);
  const [activeStage, setActiveStage] = useState(0);
  const [layers, setLayers] = useState<AtlasLayers>([true, true, true]);
  const [rotation, setRotation] = useState(0.2);
  const [reset, setReset] = useState(0);
  const [cue, setCue] = useState<Cue>({ key: "idle", id: 0 });
  const [details, setDetails] = useState(false);
  const firstSummary = useRef<HTMLElement>(null);
  const [spatial, setSpatial] = useState(false);
  const [failed, setFailed] = useState(false);
  const [ready, setReady] = useState(false);
  const sceneReady = useCallback(() => setReady(true), []);
  const sceneFailure = useCallback(() => {
    setSpatial(false);
    setReady(false);
    setFailed(true);
  }, []);
  const [visible, setVisible] = useState(true);
  // Keep browser preferences out of the server and initial hydration render.
  const [reduced, setReduced] = useState(true);
  const root = useRef<HTMLDivElement>(null);
  const expandButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!expanded) return;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setExpanded(false);
        return;
      }
      if (event.key !== "Tab") return;
      const controls = Array.from(
        root.current?.querySelectorAll<HTMLElement>(
          'a[href], button:not(:disabled), input, summary, [tabindex="0"]',
        ) ?? [],
      ).filter(
        (element) =>
          element.getClientRects().length &&
          getComputedStyle(element).visibility !== "hidden",
      );
      const first = controls[0];
      const last = controls[controls.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    expandButton.current?.focus();
    document.addEventListener("keydown", keyboard);
    return () => {
      document.body.style.overflow = previousOverflow;
      document.removeEventListener("keydown", keyboard);
      expandButton.current?.focus();
    };
  }, [expanded]);
  // One observer marks the stage crossing the middle of the dialog.
  useEffect(() => {
    const dialog = root.current;
    if (!expanded || !dialog) return;
    setActiveStage(0);
    setDetails(false);
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries)
          if (entry.isIntersecting)
            setActiveStage(Number((entry.target as HTMLElement).dataset.stage));
      },
      { root: dialog, rootMargin: "-45% 0px -50% 0px" },
    );
    dialog
      .querySelectorAll(".atlas-stage")
      .forEach((stage) => observer.observe(stage));
    return () => observer.disconnect();
  }, [expanded]);
  const stage = expanded ? tourStages[activeStage] : undefined;
  useEffect(() => {
    if (stage?.symbol)
      setSelected(examples.findIndex(({ name }) => name === stage.symbol));
    // Each stage that reaches the middle of the dialog plays its cue once.
    setCue(({ id }) => ({ key: stage?.key ?? "idle", id: id + 1 }));
  }, [stage]);
  useEffect(() => {
    if (details) firstSummary.current?.focus();
  }, [details]);
  const inView = useInView(root);
  useEffect(() => {
    const motionPreference = matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReduced(motionPreference.matches);
    update();
    motionPreference.addEventListener("change", update);
    const visibility = () => setVisible(!document.hidden);
    document.addEventListener("visibilitychange", visibility);
    return () => {
      motionPreference.removeEventListener("change", update);
      document.removeEventListener("visibilitychange", visibility);
    };
  }, []);
  // Load on approach, then retain the scene while demand rendering pauses offscreen.
  useEffect(() => {
    if (!inView || spatial || failed) return;
    try {
      const canvas = document.createElement("canvas");
      const gl = canvas.getContext("webgl2");
      if (!gl) {
        setFailed(true);
        return;
      }
      gl.getExtension("WEBGL_lose_context")?.loseContext();
      setSpatial(true);
    } catch {
      setFailed(true);
    }
  }, [inView, spatial, failed]);
  const controls = (
    <div className="spatial-controls">
      {ready && (
        <>
          <button
            type="button"
            onClick={() => setRotation((value) => value + Math.PI / 4)}
          >
            {t("rotate")} ↻
          </button>
          <button
            type="button"
            onClick={() => {
              setRotation(0.2);
              setReset((value) => value + 1);
            }}
          >
            {t("resetView")}
          </button>
        </>
      )}
      <button
        ref={expandButton}
        type="button"
        className="atlas-expand"
        onClick={() => setExpanded((value) => !value)}
        aria-label={t(expanded ? "exitFullscreen" : "fullscreen")}
      >
        {expanded ? (
          <Minimize2 size={16} aria-hidden="true" />
        ) : (
          <Maximize2 size={16} aria-hidden="true" />
        )}
        <span>{t(expanded ? "exitFullscreen" : "fullscreen")}</span>
      </button>
    </div>
  );
  return (
    <div
      className={`atlas${expanded ? " atlas-expanded" : ""}`}
      ref={root}
      data-scene={failed ? "failed" : ready ? "ready" : "loading"}
      role={expanded ? "dialog" : undefined}
      aria-modal={expanded ? true : undefined}
      aria-label={expanded ? t("atlas") : undefined}
    >
      <div className="atlas-project">
        <div>
          <span className="pack-label">{t("exampleProject")}</span>
          <strong>
            mini-consumes-tokens <span>/ Rust</span>
          </strong>
        </div>
        <p>{t("projectBrief")}</p>
        <p className="atlas-project-flow">
          {flowKeys.map((key, i) => (
            <Fragment key={key}>
              {i > 0 && <ArrowIcon size="1.25em" />}
              <span>{t(key)}</span>
            </Fragment>
          ))}
        </p>
      </div>
      <div className="atlas-toolbar">
        <div
          className="atlas-layer-toggles"
          role="group"
          aria-label={t("visibleLayers")}
        >
          {layerKeys.map((key, i) => (
            <button
              key={key}
              type="button"
              aria-pressed={layers[i]}
              onClick={() =>
                setLayers((previous) => {
                  const next: AtlasLayers = [...previous];
                  if (previous[i] && previous.filter(Boolean).length === 1)
                    return previous;
                  next[i] = !previous[i];
                  return next;
                })
              }
            >
              <span>0{i + 1}</span> {t(key)}
            </button>
          ))}
        </div>
      </div>
      <div className="atlas-view">
        <div className="atlas-space">
          {!ready && !failed && (
            <p className="atlas-loading" role="status">
              {t("loading3d")}
            </p>
          )}
          {spatial && (
            <div className="spatial-layer" data-testid="spatial-layer">
              <SceneBoundary onFailure={sceneFailure}>
                <Suspense fallback={null}>
                  <SpatialAtlas
                    selected={selected}
                    layers={layers}
                    focusLayer={stage?.layer}
                    cue={cue}
                    rotation={rotation}
                    reset={reset}
                    onSelect={setSelected}
                    onReady={sceneReady}
                    active={inView && visible}
                    reduced={reduced}
                    onFailure={sceneFailure}
                  />
                </Suspense>
              </SceneBoundary>
            </div>
          )}
          {!failed && controls}
        </div>
        {failed && (
          <p className="atlas-fallback" role="status">
            {t("fallback")}
          </p>
        )}
        {/* .atlas-space hides beside the fallback, so the row follows it instead. */}
        {failed && controls}
        {expanded && (
          <div className="atlas-tour" data-active-stage={activeStage}>
            {tourStages.map(({ key, icon: Icon, methods, tools, docs }, i) => (
              <motion.section
                key={key}
                className="atlas-stage"
                data-stage={i}
                aria-labelledby={`${key}-title`}
                initial={reduced ? false : { opacity: 0, y: 12 }}
                whileInView={{ opacity: 1, y: 0 }}
                viewport={{ once: true, root }}
              >
                <Icon size={20} aria-hidden="true" />
                <h3 id={`${key}-title`}>{t(`${key}Title`)}</h3>
                <p>{t(`${key}Text`)}</p>
                {methods && (
                  <ul className="atlas-tools">
                    {methods.map(({ name, at, link }) => (
                      <li key={name}>
                        <p className="atlas-tool-call">
                          <code>{name}</code> <span>{at}</span>
                        </p>
                        <p>{link}</p>
                      </li>
                    ))}
                  </ul>
                )}
                {tools && (
                  <ul className="atlas-tools">
                    {tools.map(({ name, params, replaces }) => (
                      <li key={name}>
                        <p className="atlas-tool-call">
                          <code>{name}</code> <span>{params}</span>
                        </p>
                        <p>{t(`ask_${name}`)}</p>
                        {replaces && (
                          <p className="atlas-tool-replaces">
                            {t("replaces")} <code>{replaces}</code>
                          </p>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
                {docs && (
                  <>
                    <button
                      type="button"
                      className="atlas-details-toggle"
                      aria-expanded={details}
                      aria-controls="atlas-details"
                      onClick={() => setDetails((value) => !value)}
                    >
                      {t(details ? "hideSymbols" : "allSymbols")}
                    </button>
                    {/* Mounted only when open, so the FAQ keeps the page's first <details>. */}
                    {details && (
                      <section
                        id="atlas-details"
                        className="atlas-details"
                        aria-labelledby="atlas-details-title"
                      >
                        <h4 id="atlas-details-title">{t("allSymbolsTitle")}</h4>
                        {fileSymbols.map(({ file, symbols }, i) => (
                          // A shared name keeps one section open at a time.
                          <details key={file} name="atlas-files">
                            <summary ref={i === 0 ? firstSummary : undefined}>
                              {file} ·{" "}
                              {t("symbolCount", { count: symbols.length })}
                            </summary>
                            <ul>
                              {symbols.map(([line, name, kind, pub]) => (
                                <li key={line}>
                                  <code>{name}</code>
                                  <span>
                                    {pub ? "pub " : ""}
                                    {kind} · {line}
                                  </span>
                                </li>
                              ))}
                            </ul>
                          </details>
                        ))}
                      </section>
                    )}
                    <div className="command-panel">
                      {setupCommands.map(({ command, label }) => (
                        <CopyCommand
                          key={command}
                          command={command}
                          label={t(label)}
                        />
                      ))}
                    </div>
                    <Link href={docs} className="text-link">
                      {t("toolsReference")}
                      <ArrowUpRight size={16} aria-hidden="true" />
                    </Link>
                  </>
                )}
              </motion.section>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
