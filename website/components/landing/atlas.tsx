"use client";
import {
  Component,
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
import { FileCode2, Maximize2, Minimize2 } from "lucide-react";

import { examples, layerKeys, type AtlasLayers } from "./atlas-data";

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
  const [layers, setLayers] = useState<AtlasLayers>([true, true, true]);
  const [separation, setSeparation] = useState(1.45);
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
          'button:not(:disabled), input, [tabindex="0"]',
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
  const item = examples[selected];
  return (
    <div
      className={`atlas${expanded ? " atlas-expanded" : ""}`}
      ref={root}
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
          <button
            ref={expandButton}
            type="button"
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
      </div>
      <div className="atlas-project">
        <div>
          <span className="pack-label">{t("exampleProject")}</span>
          <strong>
            request-service <span>/ Rust</span>
          </strong>
        </div>
        <p>{t("projectBrief")}</p>
        <p className="atlas-project-flow">
          main.rs <span>→</span> server.rs <span>→</span> parser.rs{" "}
          <span>→</span> validation.rs
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
                  separation={separation}
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
      </div>
      {failed && (
        <p className="atlas-fallback" role="status">
          {t("fallback")}
        </p>
      )}
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
              <span>{node.filename}</span>
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
              <dd>{item.file}</dd>
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
