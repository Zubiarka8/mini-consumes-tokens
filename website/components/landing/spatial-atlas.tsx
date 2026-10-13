"use client";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import { useEffect, useMemo, useRef, type RefObject } from "react";
import {
  BufferGeometry,
  Color,
  Group,
  type Mesh,
  QuadraticBezierCurve3,
  Vector3,
  PerspectiveCamera,
  type LineBasicMaterial,
  type MeshBasicMaterial,
  type MeshStandardMaterial,
} from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { useTranslation } from "react-i18next";
import { FileCode2 } from "lucide-react";

import {
  connections,
  CUE_STEP,
  examples,
  files,
  LAYER_SEPARATION,
  layerHeight,
  layerKeys,
  tourStages,
  type AtlasLayers,
  type Cue,
} from "./atlas-data";

const lit = new Color("#b0d897");
const dim = new Color("#48624f");
const resting = new Color("#899c8c");
// Moves `color` toward `target` by `amount`, snapping at the end; true while it moves.
function easeColor(color: Color, target: Color, amount: number) {
  color.lerp(target, amount);
  const moving =
    Math.abs(color.r - target.r) +
      Math.abs(color.g - target.g) +
      Math.abs(color.b - target.b) >
    0.003;
  if (!moving) color.copy(target);
  return moving;
}
function easeEmissive(
  material: MeshStandardMaterial,
  target: number,
  amount: number,
) {
  const remaining = target - material.emissiveIntensity;
  material.emissiveIntensity =
    Math.abs(remaining) < 0.001
      ? target
      : material.emissiveIntensity + remaining * amount;
  return material.emissiveIntensity !== target;
}

type Point = readonly [number, number];
function link([ax, az]: Point, [bx, bz]: Point) {
  const curve = new QuadraticBezierCurve3(
    new Vector3(ax, 0.15, az),
    new Vector3((ax + bx) / 2, 0.45, (az + bz) / 2 - 0.35),
    new Vector3(bx, 0.15, bz),
  );
  const points = curve.getPoints(40);
  const geometry = new BufferGeometry().setFromPoints(
    points.slice(0, -1).flatMap((point, i) => [point, points[i + 1]]),
  );
  return { curve, geometry };
}
// Idle motion periods in seconds, and how far the context sheets float.
const PULSE_PERIOD = 2.4;
const BREATH_PERIOD = 3;
const FLOAT_PERIOD = 4;
const FLOAT_HEIGHT = 0.02;
function CameraControls({ active, reset }: { active: boolean; reset: number }) {
  const { camera, gl, invalidate, size } = useThree();
  const controls = useRef<OrbitControls | null>(null);
  useEffect(() => {
    const perspective = camera as PerspectiveCamera;
    perspective.fov =
      (2 *
        Math.atan(
          Math.tan((43 * Math.PI) / 360) *
            Math.max(1, 760 / 510 / (size.width / size.height)),
        ) *
        180) /
      Math.PI;
    perspective.updateProjectionMatrix();
    invalidate();
  }, [camera, size.width, size.height, invalidate]);
  useEffect(() => {
    const orbit = new OrbitControls(camera, gl.domElement);
    orbit.enablePan = false;
    orbit.minDistance = 7;
    orbit.maxDistance = 14;
    orbit.minPolarAngle = Math.PI / 5;
    orbit.maxPolarAngle = (Math.PI * 3) / 4;
    orbit.saveState();
    controls.current = orbit;
    return () => {
      orbit.dispose();
      controls.current = null;
    };
  }, [camera, gl]);
  useEffect(() => {
    const orbit = controls.current;
    if (!orbit) return;
    orbit.enabled = active;
    const change = () => {
      if (active) invalidate();
    };
    orbit.addEventListener("change", change);
    return () => orbit.removeEventListener("change", change);
  }, [active, invalidate]);
  // Projected labels sit over the canvas; let scrolling over them still zoom the scene.
  useEffect(() => {
    const layer = gl.domElement.closest(".spatial-layer");
    const forward = (event: Event) => {
      if (!(event.target as Element).closest(".spatial-label")) return;
      event.preventDefault();
      gl.domElement.dispatchEvent(new WheelEvent("wheel", event as WheelEvent));
    };
    layer?.addEventListener("wheel", forward, { passive: false });
    return () => layer?.removeEventListener("wheel", forward);
  }, [gl]);
  useEffect(() => {
    controls.current?.reset();
  }, [reset]);
  return null;
}
function Graph({
  selected,
  rotation,
  reduced,
  active,
  layers,
  focusLayer,
  cue,
  labels,
  fileLabels,
  boardLabels,
  onSelect,
  onReady,
  onFailure,
}: {
  selected: number;
  rotation: number;
  reduced: boolean;
  active: boolean;
  layers: AtlasLayers;
  focusLayer?: number;
  cue: Cue;
  labels: RefObject<(HTMLButtonElement | null)[]>;
  fileLabels: RefObject<(HTMLButtonElement | null)[]>;
  boardLabels: RefObject<(HTMLSpanElement | null)[]>;
  onSelect: (index: number) => void;
  onReady: () => void;
  onFailure: () => void;
}) {
  const group = useRef<Group>(null);
  const firstFrame = useRef(true);
  const cubes = useRef<(MeshStandardMaterial | null)[]>([]);
  const boards = useRef<(MeshStandardMaterial | null)[]>([]);
  const links = useRef<(LineBasicMaterial | MeshBasicMaterial | null)[]>([]);
  const pulses = useRef<(Mesh | null)[]>([]);
  const sheets = useRef<(Group | null)[]>([]);
  const cueId = useRef(-1);
  const cueStart = useRef(0);
  const { invalidate, gl } = useThree();
  const projected = useMemo(() => new Vector3(), []);
  const curves = useMemo(
    () => connections.map(([a, b]) => link(a.position, b.position)),
    [],
  );
  // An unknown key, or "idle", holds no state: every connection stays lit.
  const steps = useMemo(
    () => tourStages.find(({ key }) => key === cue.key)?.steps,
    [cue.key],
  );
  useEffect(
    () => () => curves.forEach(({ geometry }) => geometry.dispose()),
    [curves],
  );
  useEffect(() => {
    if (active) invalidate();
  }, [active, rotation, selected, layers, focusLayer, cue, invalidate]);
  useEffect(() => {
    const lost = (event: Event) => {
      event.preventDefault();
      onFailure();
    };
    gl.domElement.addEventListener("webglcontextlost", lost);
    return () => gl.domElement.removeEventListener("webglcontextlost", lost);
  }, [gl, onFailure]);
  useFrame(({ camera, clock, size }, delta) => {
    if (!active || !group.current) return;
    const remaining = rotation - group.current.rotation.y;
    group.current.rotation.y +=
      remaining * (reduced ? 1 : Math.min(1, delta * 9));
    // Selection, focused board and the cue's connections ease toward their state.
    // On demand, the first frame after a pause reports the whole pause as its delta.
    const amount =
      reduced || firstFrame.current ? 1 : Math.min(delta, 1 / 30) * 10;
    let moving = Math.abs(remaining) > 0.001;
    // Idle motion, never on anything that carries an HTML label: a pulse along every
    // connection, breathing on the selected cube and focused board, floating sheets.
    const time = clock.elapsedTime;
    const breath = reduced
      ? 0
      : 0.1 * Math.sin((2 * Math.PI * time) / BREATH_PERIOD);
    for (let i = 0; i < connections.length; i++) {
      const pulse = pulses.current[i];
      if (!pulse) continue;
      pulse.visible = !reduced;
      curves[i].curve.getPoint(
        (time / PULSE_PERIOD + i / connections.length) % 1,
        pulse.position,
      );
    }
    for (let i = 0; i < examples.length; i++) {
      const sheet = sheets.current[i];
      if (sheet)
        sheet.position.y = reduced
          ? 0
          : FLOAT_HEIGHT * Math.sin((2 * Math.PI * time) / FLOAT_PERIOD + i);
    }
    for (let i = 0; i < examples.length; i++) {
      const cube = cubes.current[i];
      if (!cube) continue;
      if (easeColor(cube.color, selected === i ? lit : resting, amount))
        moving = true;
      if (easeEmissive(cube, selected === i ? 0.25 + breath : 0, amount))
        moving = true;
    }
    for (let i = 0; i < boards.current.length; i++) {
      const board = boards.current[i];
      if (
        board &&
        easeEmissive(board, focusLayer === i ? 0.45 + breath : 0, amount)
      )
        moving = true;
    }
    if (cueId.current !== cue.id) {
      cueId.current = cue.id;
      cueStart.current = clock.elapsedTime;
    }
    // Step k starts at k * CUE_STEP; reduced motion applies the final state at once.
    const elapsed = reduced
      ? Infinity
      : (clock.elapsedTime - cueStart.current) * 1000;
    if (steps && elapsed < (steps.length - 1) * CUE_STEP) moving = true;
    for (let i = 0; i < connections.length; i++) {
      let on = !steps;
      for (let k = 0; !on && steps && k < steps.length; k++)
        on = elapsed >= k * CUE_STEP && steps[k].includes(i);
      const line = links.current[2 * i];
      const arrow = links.current[2 * i + 1];
      if (line && easeColor(line.color, on ? lit : dim, amount)) moving = true;
      if (arrow && easeColor(arrow.color, on ? lit : dim, amount))
        moving = true;
    }
    group.current.updateWorldMatrix(true, true);
    camera.updateMatrixWorld();
    const place = (
      element: HTMLElement | null,
      x: number,
      y: number,
      z: number,
      shown: boolean,
      offset = 0,
    ) => {
      if (!element) return;
      projected.set(x, y, z);
      group.current!.localToWorld(projected).project(camera);
      element.style.left = `${((projected.x + 1) * size.width) / 2}px`;
      element.style.top = `${((1 - projected.y) * size.height) / 2 + offset}px`;
      element.style.visibility =
        shown && projected.z < 1 ? "visible" : "hidden";
    };
    examples.forEach(({ position: [x, z] }, i) =>
      place(labels.current[i], x, 0.2, z, layers[1], 16),
    );
    files.forEach(({ x }, i) =>
      place(
        fileLabels.current[i],
        x,
        LAYER_SEPARATION + 0.65,
        0,
        layers[0],
        -30,
      ),
    );
    layers.forEach((shown, i) =>
      place(boardLabels.current[i], -2.65, layerHeight(i), 1.35, shown, 8),
    );
    if (firstFrame.current) {
      firstFrame.current = false;
      onReady();
    }
    // Continuous while visible; with reduced motion, only until a change settles.
    if (!reduced || moving) invalidate();
  });
  return (
    <group ref={group} rotation={[0, 0.2, 0]}>
      {layers.map(
        (shown, layer) =>
          shown && (
            <group key={layer} position={[0, layerHeight(layer), 0]}>
              <mesh position={[0, -0.09, 0]}>
                <boxGeometry args={[5.5, 0.14, 2.7]} />
                <meshStandardMaterial
                  ref={(material) => {
                    boards.current[layer] = material;
                  }}
                  color={layer === 1 ? "#334f3c" : "#405246"}
                  emissive="#60814b"
                  emissiveIntensity={0}
                  roughness={0.6}
                  metalness={0.06}
                />
              </mesh>
              {/* Fine board dividers keep the layers legible without decorative motion. */}
              {[-0.9, 0.9].map((x) => (
                <mesh key={x} position={[x, -0.01, 0]}>
                  <boxGeometry args={[0.012, 0.01, 2.5]} />
                  <meshBasicMaterial color="#48624f" />
                </mesh>
              ))}
              {layer === 0 &&
                files.map(({ x }, file) => {
                  const select = (event: { stopPropagation: () => void }) => {
                    event.stopPropagation();
                    onSelect(examples.findIndex((item) => item.file === file));
                  };
                  return (
                    <group key={file} position={[x, 0.32, 0]}>
                      <mesh onClick={select}>
                        <boxGeometry args={[0.4, 0.62, 0.11]} />
                        <meshStandardMaterial
                          color={
                            examples[selected].file === file
                              ? "#b0d897"
                              : "#899c8c"
                          }
                          roughness={0.35}
                          metalness={0.1}
                        />
                      </mesh>
                      {[0.12, 0.02, -0.08].map((y) => (
                        <mesh key={y} position={[0, y, 0.061]}>
                          <boxGeometry args={[0.23, 0.018, 0.009]} />
                          <meshBasicMaterial color="#3c5842" />
                        </mesh>
                      ))}
                      <mesh position={[0.115, 0.22, 0.065]}>
                        <boxGeometry args={[0.11, 0.1, 0.012]} />
                        <meshStandardMaterial color="#dce6d4" />
                      </mesh>
                    </group>
                  );
                })}
              {layer > 0 &&
                examples.map(({ position: [x, z] }, i) => {
                  const select = (event: { stopPropagation: () => void }) => {
                    event.stopPropagation();
                    onSelect(i);
                  };
                  return (
                    <group key={i} position={[x, 0, z]}>
                      {layer === 1 ? (
                        <mesh position={[0, 0.16, 0]} onClick={select}>
                          <boxGeometry args={[0.32, 0.32, 0.32]} />
                          {/* Colour and emissive ease in the frame loop. */}
                          <meshStandardMaterial
                            ref={(material) => {
                              cubes.current[i] = material;
                            }}
                            color="#899c8c"
                            emissive="#60814b"
                            emissiveIntensity={0}
                            roughness={0.35}
                          />
                        </mesh>
                      ) : (
                        <group
                          ref={(element) => {
                            sheets.current[i] = element;
                          }}
                        >
                          {[0, 1, 2, 3].map((sheet) => (
                            <mesh
                              key={sheet}
                              position={[0, 0.04 + sheet * 0.045, 0]}
                              onClick={select}
                            >
                              <boxGeometry args={[0.6, 0.032, 0.5]} />
                              <meshStandardMaterial
                                color={selected === i ? "#b0d897" : "#899c8c"}
                                roughness={0.7}
                              />
                            </mesh>
                          ))}
                          {[-0.14, -0.03, 0.08, 0.19].map((z) => (
                            <mesh key={z} position={[0, 0.2, z]}>
                              <boxGeometry args={[0.42, 0.007, 0.022]} />
                              <meshBasicMaterial color="#507047" />
                            </mesh>
                          ))}
                        </group>
                      )}
                    </group>
                  );
                })}
              {layer === 1 &&
                connections.map(([a, b], i) => {
                  const [ax, az] = a.position;
                  const [bx, bz] = b.position;
                  return (
                    <group key={i}>
                      <mesh
                        ref={(element) => {
                          pulses.current[i] = element;
                        }}
                      >
                        <sphereGeometry args={[0.045, 12, 8]} />
                        <meshBasicMaterial color="#dce6d4" />
                      </mesh>
                      <lineSegments geometry={curves[i].geometry}>
                        <lineBasicMaterial
                          ref={(material) => {
                            links.current[2 * i] = material;
                          }}
                          color="#b0d897"
                        />
                      </lineSegments>
                      <mesh
                        position={[
                          bx + (ax - bx) * 0.12,
                          0.2,
                          bz + (az - bz) * 0.12,
                        ]}
                        rotation={[
                          Math.PI / 2,
                          0,
                          Math.atan2(ax - bx, bz - az),
                        ]}
                      >
                        <coneGeometry args={[0.055, 0.16, 8]} />
                        <meshBasicMaterial
                          ref={(material) => {
                            links.current[2 * i + 1] = material;
                          }}
                          color="#b0d897"
                        />
                      </mesh>
                    </group>
                  );
                })}
            </group>
          ),
      )}
      {layers.map((shown, layer) => {
        if (!shown || layer === 2 || !layers[layer + 1]) return null;
        const [x, z] = examples[selected].position;
        const top = layerHeight(layer);
        const bottom = layerHeight(layer + 1);
        return (
          <group key={layer}>
            {Array.from({ length: 9 }, (_, i) => (
              <mesh
                key={i}
                position={[x, bottom + ((top - bottom) * (i + 0.5)) / 9, z]}
              >
                <boxGeometry args={[0.018, (top - bottom) / 18, 0.018]} />
                <meshBasicMaterial color="#b0d897" />
              </mesh>
            ))}
          </group>
        );
      })}
    </group>
  );
}
export default function SpatialAtlas({
  selected,
  layers,
  focusLayer,
  rotation,
  reset,
  active,
  reduced,
  cue,
  onSelect,
  onReady,
  onFailure,
}: {
  selected: number;
  layers: AtlasLayers;
  focusLayer?: number;
  rotation: number;
  reset: number;
  active: boolean;
  reduced: boolean;
  cue: Cue;
  onSelect: (index: number) => void;
  onReady: () => void;
  onFailure: () => void;
}) {
  const { t } = useTranslation();
  const fileLabels = useRef<(HTMLButtonElement | null)[]>([]);
  const boardLabels = useRef<(HTMLSpanElement | null)[]>([]);
  const labels = useRef<(HTMLButtonElement | null)[]>([]);
  return (
    <>
      <Canvas
        aria-hidden="true"
        tabIndex={-1}
        camera={{ position: [4.6, 4.2, 8], fov: 43 }}
        dpr={[1, 2]}
        // Pointer events use offset coordinates and labels only need the size, so
        // skip the default scroll listeners that re-measure the canvas on every scroll.
        resize={{ scroll: false }}
        frameloop={active ? "demand" : "never"}
        gl={{ antialias: true, alpha: true, powerPreference: "low-power" }}
        fallback={<span>{t("fallback")}</span>}
      >
        <ambientLight intensity={1.5} />
        <directionalLight position={[3, 4, 5]} intensity={3} />
        <CameraControls active={active} reset={reset} />
        <Graph
          selected={selected}
          rotation={rotation}
          reduced={reduced}
          active={active}
          labels={labels}
          fileLabels={fileLabels}
          boardLabels={boardLabels}
          layers={layers}
          focusLayer={focusLayer}
          cue={cue}
          onSelect={onSelect}
          onReady={onReady}
          onFailure={onFailure}
        />
      </Canvas>
      <div className="spatial-labels" role="group" aria-label={t("select")}>
        {examples.map(({ name }, i) => (
          <button
            key={name}
            type="button"
            className="spatial-label"
            ref={(element) => {
              labels.current[i] = element;
            }}
            aria-pressed={selected === i}
            onClick={() => onSelect(i)}
          >
            {t(`role${i}`)}
          </button>
        ))}
      </div>
      <div className="spatial-file-labels">
        {files.map(({ name, path }, i) => (
          <button
            key={name}
            type="button"
            className="spatial-label spatial-file-label"
            ref={(element) => {
              fileLabels.current[i] = element;
            }}
            aria-label={`${t("inspectFile")} ${path}`}
            aria-pressed={examples[selected].file === i}
            onClick={() =>
              onSelect(examples.findIndex(({ file }) => file === i))
            }
          >
            <FileCode2 size={12} aria-hidden="true" /> {name}
          </button>
        ))}
      </div>
      <div className="spatial-board-labels" aria-hidden="true">
        {layerKeys.map((key, i) => (
          <span
            key={key}
            ref={(element) => {
              boardLabels.current[i] = element;
            }}
          >
            0{i + 1} / {t(key)}
          </span>
        ))}
      </div>
      <p className="spatial-hint">{t("spatialHint")}</p>
    </>
  );
}
