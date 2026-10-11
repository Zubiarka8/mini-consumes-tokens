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
import { ArrowUpRight, FileCode2, Maximize2, Minimize2 } from "lucide-react";

import {
  examples,
  files,
  layerKeys,
  storyStages,
  tourStages,
  type AtlasLayers,
  type Cue,
} from "./atlas-data";
import { fileSymbols } from "./atlas-symbols";
import { CopyCommand, setupCommands } from "./copy-command";

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
  const [storyStage, setStoryStage] = useState(0);
  const [layers, setLayers] = useState<AtlasLayers>([true, true, true]);
  const [separation, setSeparation] = useState(1.45);
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
  // One observer marks the stage crossing the middle of the dialog in full screen, or
  // the story stage crossing the middle of the viewport on the page.
  useEffect(() => {
    const atlas = root.current;
    if (!atlas) return;
    if (expanded) {
      setActiveStage(0);
      setDetails(false);
    }
    const setStage = expanded ? setActiveStage : setStoryStage;
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries)
          if (entry.isIntersecting)
            setStage(Number((entry.target as HTMLElement).dataset.stage));
      },
      { root: expanded ? atlas : null, rootMargin: "-45% 0px -50% 0px" },
    );
    atlas
      .querySelectorAll(expanded ? ".atlas-stage" : ".atlas-story-stage")
      .forEach((stage) => observer.observe(stage));
    return () => observer.disconnect();
  }, [expanded]);
  const tourStage = expanded ? tourStages[activeStage] : undefined;
  const story = storyStages[storyStage];
  const stage = tourStage ?? story;
  // A stage selects its symbol when it activates; leaving full screen keeps the
  // selection made there, because the story stage itself has not changed.
  useEffect(() => {
    if (tourStage?.symbol)
      setSelected(examples.findIndex(({ name }) => name === tourStage.symbol));
  }, [tourStage]);
  useEffect(() => {
    if (story.symbol)
      setSelected(examples.findIndex(({ name }) => name === story.symbol));
  }, [story]);
  // Each stage that reaches the middle plays its cue once.
  useEffect(() => {
    setCue(({ id }) => ({ key: stage.key, id: id + 1 }));
  }, [stage]);
  useEffect(() => {
    if (details) firstSummary.current?.focus();
  }, [details]);
  // The scene, not the whole atlas: the story makes the atlas much taller than its scene.
  const space = useRef<HTMLDivElement>(null);
  const inView = useInView(space);
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
  const item = examples[selected];
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
      data-story-stage={expanded ? undefined : storyStage}
      role={expanded ? "dialog" : undefined}
      aria-modal={expanded ? true : undefined}
      aria-label={expanded ? t("atlas") : undefined}
    >
      <div className="atlas-top">
        <span>
          <span className="status-dot" />
          {t("atlas")}
        </span>
        <div className="atlas-top-actions">
          <span className="mono">MCT / 001</span>
        </div>
      </div>
      <div className="atlas-project">
        <div>
          <span className="pack-label">{t("exampleProject")}</span>
          <strong>
            mini-consumes-tokens <span>/ Rust</span>
          </strong>
        </div>
        <p>{t("projectBrief")}</p>
        <p className="atlas-project-flow">
          {files.map(({ name }, i) => (
            <Fragment key={name}>
              {i > 0 && <span> → </span>}
              {name}
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
        <label className="atlas-separation">
          <span>{t("layerSpacing")}</span>
          <input
            type="range"
            min="0.7"
            max="1.65"
            step="0.05"
            value={separation}
            onChange={(event) => setSeparation(Number(event.target.value))}
          />
        </label>
      </div>
      <div className="atlas-view">
        <div className="atlas-space" ref={space}>
          {!ready && !failed && (
            <p className="atlas-loading" role="status">
              {t("loading3d")}
            </p>
          )}
          {spatial && (
            <div
              className="spatial-layer"
              data-testid="spatial-layer"
              data-cue={cue.key}
            >
              <SceneBoundary onFailure={sceneFailure}>
                <Suspense fallback={null}>
                  <SpatialAtlas
                    selected={selected}
                    layers={layers}
                    focusLayer={stage.layer}
                    view={tourStage ? undefined : story.view}
                    separation={separation}
                    cue={cue}
                    rotation={rotation}
                    reset={reset}
                    onSelect={setSelected}
                    onReady={sceneReady}
                    active={(inView || expanded) && visible}
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
        {/* Full screen has its own tour, so the story lives only on the page. */}
        {!expanded && (
          <ol className="atlas-story">
            {storyStages.map(({ key }, i) => (
              <li
                key={key}
                className="atlas-story-stage"
                data-stage={i}
                aria-current={storyStage === i ? "step" : undefined}
              >
                <span className="step-number mono">0{i + 1}</span>
                <h3>{t(`${key}Title`)}</h3>
                <p>{t(`${key}Text`)}</p>
              </li>
            ))}
          </ol>
        )}
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
      <div className="symbol-controls" role="group" aria-label={t("select")}>
        {examples.map((node, i) => (
          <button
            key={node.name}
            type="button"
            aria-pressed={selected === i}
            aria-label={node.name}
            onClick={() => setSelected(i)}
          >
            <span className="atlas-file-name">
              <FileCode2 size={16} aria-hidden="true" />
              <span>{files[node.file].name}</span>
            </span>
            <span className="atlas-file-role">{t(`role${i}`)}</span>
            <span className="atlas-file-symbol">{node.name}</span>
          </button>
        ))}
      </div>
      <div className="context-pack" aria-live="polite" aria-atomic="true">
        <div className="atlas-query">
          <span>{t("agentQuestion")}</span>
          <p>{t(`query${selected}`)}</p>
        </div>
        <div className="pack-label">
          <span>{t("pack")}</span>
          <span aria-hidden="true">↗</span>
        </div>
        <motion.div key={item.name} initial={false} animate={{ opacity: 1 }}>
          <strong>{item.name}</strong>
          <p className="atlas-explanation">{t(`purpose${selected}`)}</p>
          <dl>
            <div>
              <dt>{t("definition")}</dt>
              <dd>
                {files[item.file].path}:{item.line}
              </dd>
            </div>
            <div>
              <dt>{t("callers")}</dt>
              <dd>{item.callers}</dd>
            </div>
            <div>
              <dt>{t("dependencies")}</dt>
              <dd>{item.calls}</dd>
            </div>
          </dl>
          <div
            className="atlas-source"
            role="region"
            aria-label={t("sourcePreview")}
            tabIndex={0}
          >
            <pre>
              <code>
                {item.source.map((line, i) => (
                  <span className="atlas-code-line" key={i}>
                    <span className="atlas-line-number" aria-hidden="true">
                      {item.line + i}
                    </span>
                    <span>{line}</span>
                  </span>
                ))}
              </code>
            </pre>
          </div>
        </motion.div>
      </div>
      <p className="atlas-caption">{t("illustration")}</p>
    </div>
  );
}
