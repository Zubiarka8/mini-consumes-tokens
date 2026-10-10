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
import { Box, Layers2 } from "lucide-react";

const SpatialAtlas = lazy(() => import("./spatial-atlas"));
export const examples = [
  {
    name: "parse_request",
    file: "src/parser.rs:18",
    callers: "handle_request",
    calls: "validate_input",
    x: 245,
    y: 135,
  },
  {
    name: "handle_request",
    file: "src/server.rs:42",
    callers: "route",
    calls: "parse_request",
    x: 380,
    y: 250,
  },
  {
    name: "validate_input",
    file: "src/validation.rs:9",
    callers: "parse_request",
    calls: "normalize",
    x: 160,
    y: 300,
  },
];
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
  const [spatial, setSpatial] = useState(false);
  const [eligible, setEligible] = useState(false);
  const [mounted, setMounted] = useState(false);
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
  const inView = useInView(root);
  useEffect(() => {
    const media = matchMedia("(min-width: 768px)");
    const motionPreference = matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => {
      setEligible(media.matches);
      setReduced(motionPreference.matches);
      if (!media.matches || motionPreference.matches) {
        setSpatial(false);
        setReady(false);
      }
    };
    update();
    setMounted(true);
    media.addEventListener("change", update);
    motionPreference.addEventListener("change", update);
    const visibility = () => setVisible(!document.hidden);
    document.addEventListener("visibilitychange", visibility);
    return () => {
      media.removeEventListener("change", update);
      motionPreference.removeEventListener("change", update);
      document.removeEventListener("visibilitychange", visibility);
    };
  }, []);
  function toggleSpatial() {
    if (spatial) {
      setSpatial(false);
      setReady(false);
      return;
    }
    try {
      const canvas = document.createElement("canvas");
      const gl = canvas.getContext("webgl2");
      if (!gl) {
        setFailed(true);
        return;
      }
      gl.getExtension("WEBGL_lose_context")?.loseContext();
      setFailed(false);
      setReady(false);
      setSpatial(true);
    } catch {
      setFailed(true);
    }
  }
  const use3d = spatial && eligible && reduced === false;
  const item = examples[selected];
  return (
    <div className="atlas" ref={root}>
      <div className="atlas-top">
        <span>
          <span className="status-dot" />
          {t("atlas")}
        </span>
        <span className="mono">MCT / 001</span>
      </div>
      <div className="atlas-stages">
        <span>01 {t("files")}</span>
        <span>02 {t("symbols")}</span>
        <span>03 {t("context")}</span>
      </div>
      <div className="atlas-space">
        <svg
          className="atlas-svg"
          viewBox="0 0 540 400"
          role="img"
          aria-label={t("graphAlt")}
          style={{ visibility: use3d && ready ? "hidden" : "visible" }}
        >
          <defs>
            <pattern
              id="atlas-grid"
              width="30"
              height="30"
              patternUnits="userSpaceOnUse"
            >
              <circle cx="1" cy="1" r="1" fill="#647568" opacity=".5" />
            </pattern>
          </defs>
          <rect width="540" height="400" fill="url(#atlas-grid)" />
          <g fill="none" stroke="#627969" strokeWidth="1">
            <ellipse
              cx="270"
              cy="215"
              rx="213"
              ry="125"
              transform="rotate(-18 270 215)"
              opacity=".4"
            />
            <ellipse
              cx="270"
              cy="215"
              rx="150"
              ry="175"
              transform="rotate(30 270 215)"
              opacity=".25"
            />
            <path d="M70 100L245 135L380 250L460 140M245 135L160 300L380 250L410 355M160 300L70 100M245 135L310 50M160 300L80 345" />
          </g>
          <motion.path
            d={`M${item.x} ${item.y}L${examples[(selected + 1) % 3].x} ${examples[(selected + 1) % 3].y}`}
            stroke="#b0d897"
            strokeWidth="2"
            fill="none"
            initial={false}
            animate={{ pathLength: 1 }}
            key={selected}
            transition={{ duration: reduced ? 0 : 0.45 }}
          />
          {[
            [70, 100],
            [460, 140],
            [410, 355],
            [310, 50],
            [80, 345],
          ].map(([x, y]) => (
            <g key={x} transform={`translate(${x} ${y})`}>
              <rect
                x="-9"
                y="-12"
                width="18"
                height="24"
                rx="2"
                fill="#202e26"
                stroke="#748979"
              />
              <path d="M-4 -3H4M-4 2H4M-4 7H1" stroke="#748979" />
            </g>
          ))}
          {examples.map((node, i) => (
            <g key={node.name}>
              <motion.circle
                cx={node.x}
                cy={node.y}
                r={i === selected ? 19 : 10}
                fill={i === selected ? "#b0d897" : "#273e30"}
                stroke="#b0d897"
                initial={false}
                animate={{ r: i === selected ? 19 : 10 }}
                transition={{ duration: reduced ? 0 : 0.25 }}
              />
              <circle cx={node.x} cy={node.y} r="3" fill="#111815" />
              <text
                x={node.x}
                y={node.y + 37}
                textAnchor="middle"
                fill={i === selected ? "#e8f4df" : "#a9b9aa"}
                fontSize="12"
                fontFamily="monospace"
              >
                {node.name}
              </text>
            </g>
          ))}
          <text
            x="18"
            y="382"
            fill="#a9b9aa"
            fontSize="11"
            fontFamily="monospace"
          >
            src/ → AST → SQLite
          </text>
        </svg>
        {use3d && (
          <div className="spatial-layer" data-testid="spatial-layer">
            <SceneBoundary onFailure={sceneFailure}>
              <Suspense fallback={<p role="status">{t("loading3d")}</p>}>
                <SpatialAtlas
                  selected={selected}
                  symbols={examples.map((example) => example.name)}
                  onSelect={setSelected}
                  onReady={sceneReady}
                  active={inView && visible}
                  onFailure={sceneFailure}
                />
              </Suspense>
            </SceneBoundary>
          </div>
        )}
        <div className="atlas-mode">
          {mounted && (
            <button
              type="button"
              onClick={toggleSpatial}
              disabled={!eligible || reduced !== false}
              aria-pressed={use3d}
            >
              {use3d ? (
                <Layers2 size={15} aria-hidden />
              ) : (
                <Box size={15} aria-hidden />
              )}
              {t(use3d ? "view2d" : "view3d")}
            </button>
          )}
        </div>
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
            onClick={() => setSelected(i)}
          >
            {node.name}
          </button>
        ))}
      </div>
      <div className="context-pack" aria-live="polite" aria-atomic="true">
        <div className="pack-label">
          <span>{t("pack")}</span>
          <span aria-hidden="true">↗</span>
        </div>
        <motion.div key={item.name} initial={false} animate={{ opacity: 1 }}>
          <strong>{item.name}</strong>
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
        </motion.div>
      </div>
      <p className="atlas-caption">{t("illustration")}</p>
    </div>
  );
}
