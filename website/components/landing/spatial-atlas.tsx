"use client";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import {
  BufferGeometry,
  Color,
  Group,
  type LineSegments,
  type Mesh,
  QuadraticBezierCurve3,
  Spherical,
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
  layerHeight,
  layerKeys,
  storyStages,
  tourStages,
  type AtlasLayers,
  type Cue,
  type View,
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

// Slow, fast, slow: ease-in-out quint, close to the references' cubic-bezier(0.84, 0, 0.16, 1).
function glide(t: number) {
  return t < 0.5 ? 16 * t ** 5 : 1 - (-2 * t + 2) ** 5 / 2;
}
// Back-out: passes its target by about 10% before settling, for hover.
function overshoot(t: number) {
  return 1 + 2.70158 * (t - 1) ** 3 + 1.70158 * (t - 1) ** 2;
}
function progress(elapsed: number, duration: number) {
  return Math.min(1, Math.max(0, elapsed / duration));
}
// Moves `value` toward `target` by at most `step`.
function approach(value: number, target: number, step: number) {
  return value < target
    ? Math.min(target, value + step)
    : Math.max(target, value - step);
}

const SEGMENTS = 40;
type Point = readonly [number, number];
function link([ax, az]: Point, [bx, bz]: Point) {
  const curve = new QuadraticBezierCurve3(
    new Vector3(ax, 0.15, az),
    new Vector3((ax + bx) / 2, 0.45, (az + bz) / 2 - 0.35),
    new Vector3(bx, 0.15, bz),
  );
  const points = curve.getPoints(SEGMENTS);
  const geometry = new BufferGeometry().setFromPoints(
    points.slice(0, -1).flatMap((point, i) => [point, points[i + 1]]),
  );
  // The lit trace shares the vertices and draws only the part already travelled.
  const trace = new BufferGeometry();
  trace.setAttribute("position", geometry.getAttribute("position"));
  return { curve, geometry, trace };
}
// Idle motion periods in seconds, and how far the context sheets float.
const BREATH_PERIOD = 3;
const FLOAT_PERIOD = 4;
const FLOAT_HEIGHT = 0.02;
// Choreography, in seconds: camera moves between stages, the boards unfold one after
// another on entry, the selection's column drops, hover grows, the context pack fans out.
const VIEW_DURATION = 1.1;
const UNFOLD_DURATION = 0.9;
const UNFOLD_STAGGER = 0.12;
const DROP_DURATION = 0.7;
const HOVER_DURATION = 0.35;
const FAN_DURATION = 0.6;
// How far, in scene units, the other boards move away in a story stage's close view.
const SPREAD = 0.7;
// Milliseconds a cue takes to draw one connection, like CUE_STEP.
const TRACE_DURATION = 520;
// The starting pose; stages without a view, and full screen, return to it.
const START: [number, number, number] = [4.6, 4.2, 8];
const startView = new Spherical().setFromCartesianCoords(...START);
// Hover keys: a file, a symbol's cube or its context sheets.
const FILE_KEY = 0;
const CUBE_KEY = 10;
const SHEET_KEY = 20;
function CameraControls({
  active,
  reset,
  reduced,
  view,
  focusLayer,
  separation,
}: {
  active: boolean;
  reset: number;
  reduced: boolean;
  view?: View;
  focusLayer?: number;
  separation: number;
}) {
  const { camera, gl, invalidate, size } = useThree();
  const controls = useRef<OrbitControls | null>(null);
  // Orbit around the target from `from` to `to`; `start` < 0 when no move is running.
  const move = useMemo(
    () => ({
      pending: true,
      start: -1,
      from: new Spherical(),
      fromY: 0,
      to: new Spherical(),
      toY: 0,
    }),
    [],
  );
  const offset = useMemo(() => new Vector3(), []);
  const spacing = useRef(separation);
  useEffect(() => {
    spacing.current = separation;
  }, [separation]);
  // Full-screen tour stages carry no view, so they never take the camera from the user.
  const anchor = view ? focusLayer : undefined;
  useEffect(() => {
    move.pending = true;
    invalidate();
  }, [view, anchor, reset, move, invalidate]);
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
    // A drag or zoom takes over from a running move until the next stage or reset.
    const takeOver = () => {
      move.pending = false;
      move.start = -1;
    };
    orbit.addEventListener("start", takeOver);
    controls.current = orbit;
    return () => {
      orbit.removeEventListener("start", takeOver);
      orbit.dispose();
      controls.current = null;
    };
  }, [camera, gl, move]);
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
  // Runs before the graph's frame, which projects the labels from this camera.
  useFrame(({ clock }) => {
    const orbit = controls.current;
    if (!active || !orbit) return;
    const { from, to } = move;
    if (move.pending) {
      move.pending = false;
      move.start = clock.elapsedTime;
      from.setFromVector3(offset.subVectors(camera.position, orbit.target));
      move.fromY = orbit.target.y;
      if (view) to.set(view.distance, view.polar, view.azimuth);
      else to.copy(startView);
      move.toY =
        view && focusLayer !== undefined
          ? layerHeight(focusLayer, spacing.current) + (view.lift ?? 0)
          : 0;
      // The shorter way round.
      to.theta =
        from.theta +
        Math.atan2(
          Math.sin(to.theta - from.theta),
          Math.cos(to.theta - from.theta),
        );
    }
    if (move.start < 0) return;
    const t = reduced
      ? 1
      : progress(clock.elapsedTime - move.start, VIEW_DURATION);
    const eased = glide(t);
    offset.setFromSphericalCoords(
      from.radius + (to.radius - from.radius) * eased,
      from.phi + (to.phi - from.phi) * eased,
      from.theta + (to.theta - from.theta) * eased,
    );
    orbit.target.set(0, move.fromY + (move.toY - move.fromY) * eased, 0);
    camera.position.copy(orbit.target).add(offset);
    orbit.update();
    if (t < 1) invalidate();
    else move.start = -1;
  });
  return null;
}
function Graph({
  selected,
  rotation,
  reduced,
  active,
  layers,
  focusLayer,
  separation,
  cue,
  apart,
  hovered,
  labels,
  fileLabels,
  boardLabels,
  onSelect,
  onHover,
  onReady,
  onFailure,
}: {
  selected: number;
  rotation: number;
  reduced: boolean;
  active: boolean;
  layers: AtlasLayers;
  focusLayer?: number;
  separation: number;
  cue: Cue;
  apart: boolean;
  hovered: number;
  labels: RefObject<(HTMLButtonElement | null)[]>;
  fileLabels: RefObject<(HTMLButtonElement | null)[]>;
  boardLabels: RefObject<(HTMLSpanElement | null)[]>;
  onSelect: (index: number) => void;
  onHover: (key: number) => void;
  onReady: () => void;
  onFailure: () => void;
}) {
  const tilt = useRef<Group>(null);
  const group = useRef<Group>(null);
  const firstFrame = useRef(true);
  const boardGroups = useRef<(Group | null)[]>([]);
  const columns = useRef<(Group | null)[]>([]);
  const fileGroups = useRef<(Group | null)[]>([]);
  const cubeMeshes = useRef<(Mesh | null)[]>([]);
  const cubes = useRef<(MeshStandardMaterial | null)[]>([]);
  const boards = useRef<(MeshStandardMaterial | null)[]>([]);
  const links = useRef<(LineBasicMaterial | MeshBasicMaterial | null)[]>([]);
  const traces = useRef<(LineSegments | null)[]>([]);
  const pulses = useRef<(Mesh | null)[]>([]);
  const sheets = useRef<(Group | null)[]>([]);
  const sheetMeshes = useRef<(Mesh | null)[]>([]);
  const cueId = useRef(-1);
  const cueStart = useRef(0);
  const dropStart = useRef(0);
  const droppedFor = useRef(-1);
  // Null until the first frame; minus infinity once reduced motion skips the entry.
  const unfoldStart = useRef<number | null>(null);
  const spacing = useRef(separation);
  const spreading = useRef(0);
  // Pointer position over the scene, -1 to 1 from its centre; null when outside.
  const pointer = useRef<{ x: number; y: number } | null>(null);
  const { invalidate, gl } = useThree();
  const projected = useMemo(() => new Vector3(), []);
  const heights = useMemo(() => new Float32Array(layers.length), [layers]);
  const hover = useMemo(
    () => new Float32Array(SHEET_KEY + examples.length),
    [],
  );
  const fan = useMemo(() => new Float32Array(examples.length), []);
  const curves = useMemo(
    () => connections.map(([a, b]) => link(a.position, b.position)),
    [],
  );
  // An unknown key, or "idle", holds no state: every connection stays lit.
  const steps = useMemo(
    () =>
      [...tourStages, ...storyStages].find(({ key }) => key === cue.key)?.steps,
    [cue.key],
  );
  // The cue step that draws each connection, or -1 when it stays dim.
  const firstStep = useMemo(
    () =>
      connections.map((_, i) =>
        steps ? steps.findIndex((step) => step.includes(i)) : -1,
      ),
    [steps],
  );
  useEffect(
    () => () =>
      curves.forEach(({ geometry, trace }) => {
        geometry.dispose();
        trace.dispose();
      }),
    [curves],
  );
  useEffect(() => {
    if (active) invalidate();
  }, [
    active,
    rotation,
    selected,
    layers,
    focusLayer,
    separation,
    cue,
    hovered,
    invalidate,
  ]);
  useEffect(() => {
    gl.domElement.style.cursor = hovered >= 0 ? "pointer" : "";
  }, [gl, hovered]);
  useEffect(() => {
    const lost = (event: Event) => {
      event.preventDefault();
      onFailure();
    };
    gl.domElement.addEventListener("webglcontextlost", lost);
    return () => gl.domElement.removeEventListener("webglcontextlost", lost);
  }, [gl, onFailure]);
  // The scene leans toward a fine pointer; the layer also holds the labels, so moving
  // onto one keeps the lean.
  useEffect(() => {
    const layer = gl.domElement.closest<HTMLElement>(".spatial-layer");
    if (!layer || !matchMedia("(hover: hover) and (pointer: fine)").matches)
      return;
    const follow = (event: PointerEvent) => {
      const box = layer.getBoundingClientRect();
      const x = ((event.clientX - box.left) / box.width) * 2 - 1;
      const y = ((event.clientY - box.top) / box.height) * 2 - 1;
      if (pointer.current) {
        pointer.current.x = x;
        pointer.current.y = y;
      } else pointer.current = { x, y };
      invalidate();
    };
    const leave = () => {
      pointer.current = null;
      invalidate();
    };
    layer.addEventListener("pointermove", follow);
    layer.addEventListener("pointerleave", leave);
    return () => {
      layer.removeEventListener("pointermove", follow);
      layer.removeEventListener("pointerleave", leave);
    };
  }, [gl, invalidate]);
  useFrame(({ camera, clock, size }, delta) => {
    if (!active || !group.current || !tilt.current) return;
    const time = clock.elapsedTime;
    const remaining = rotation - group.current.rotation.y;
    group.current.rotation.y +=
      remaining * (reduced ? 1 : Math.min(1, delta * 9));
    // Selection, focused board and the cue's connections ease toward their state.
    // On demand, the first frame after a pause reports the whole pause as its delta.
    const frame = Math.min(delta, 1 / 30);
    const amount = reduced || firstFrame.current ? 1 : frame * 10;
    let moving = Math.abs(remaining) > 0.001;
    // Entry: the stack rises and turns in while the boards unfold top to bottom.
    if (unfoldStart.current === null) unfoldStart.current = time;
    if (reduced) unfoldStart.current = -Infinity;
    const since = time - unfoldStart.current;
    const entry = glide(
      progress(since, UNFOLD_DURATION + (layers.length - 1) * UNFOLD_STAGGER),
    );
    if (entry < 1) moving = true;
    group.current.position.y = -0.5 * (1 - entry);
    group.current.scale.setScalar(0.9 + 0.1 * entry);
    spacing.current += (separation - spacing.current) * amount;
    if (Math.abs(separation - spacing.current) < 0.001)
      spacing.current = separation;
    else moving = true;
    // A story stage's close view slides the other boards away from the focused one.
    const spreadTarget = apart ? 1 : 0;
    spreading.current = approach(
      spreading.current,
      spreadTarget,
      reduced ? 1 : frame / VIEW_DURATION,
    );
    if (spreading.current !== spreadTarget) moving = true;
    const spread = SPREAD * glide(spreading.current);
    for (let i = 0; i < layers.length; i++) {
      const unfold = glide(
        progress(since - i * UNFOLD_STAGGER, UNFOLD_DURATION),
      );
      heights[i] =
        layerHeight(i, spacing.current) * (0.16 + 0.84 * unfold) +
        Math.sign((focusLayer ?? i) - i) * spread;
      boardGroups.current[i]?.position.set(0, heights[i], 0);
    }
    // The lean toward the pointer; none with reduced motion or a coarse pointer.
    const lean = reduced ? null : pointer.current;
    const leanX = lean ? lean.y * 0.04 : 0;
    const leanY = (lean ? lean.x * 0.07 : 0) - 0.6 * (1 - entry);
    tilt.current.rotation.x += (leanX - tilt.current.rotation.x) * amount * 0.6;
    tilt.current.rotation.y += (leanY - tilt.current.rotation.y) * amount * 0.6;
    if (
      Math.abs(leanX - tilt.current.rotation.x) +
        Math.abs(leanY - tilt.current.rotation.y) >
      0.0005
    )
      moving = true;
    else tilt.current.rotation.set(leanX, leanY, 0);
    // Idle motion, never on anything that carries an HTML label: breathing on the
    // selected cube and focused board, floating sheets.
    const breath = reduced
      ? 0
      : 0.1 * Math.sin((2 * Math.PI * time) / BREATH_PERIOD);
    // Hover grows a file, cube or sheet stack past its size before it settles.
    const hoverStep = reduced ? 1 : frame / HOVER_DURATION;
    for (let key = 0; key < hover.length; key++) {
      hover[key] = approach(hover[key], hovered === key ? 1 : 0, hoverStep);
      if (hover[key] !== (hovered === key ? 1 : 0)) moving = true;
    }
    const fanStep = reduced ? 1 : frame / FAN_DURATION;
    for (let i = 0; i < examples.length; i++) {
      const sheet = sheets.current[i];
      // The selected symbol's sheets fan out on the context stage.
      const target = focusLayer === 2 && selected === i ? 1 : 0;
      fan[i] = approach(fan[i], target, fanStep);
      if (fan[i] !== target) moving = true;
      const opened = glide(fan[i]);
      for (let s = 0; s < 4; s++) {
        const mesh = sheetMeshes.current[4 * i + s];
        if (!mesh) continue;
        mesh.position.set(
          s * 0.05 * opened,
          0.04 + s * 0.045 + s * 0.11 * opened,
          0,
        );
        mesh.rotation.y = s * 0.18 * opened;
      }
      if (!sheet) continue;
      sheet.position.y = reduced
        ? 0
        : FLOAT_HEIGHT * Math.sin((2 * Math.PI * time) / FLOAT_PERIOD + i);
      sheet.scale.setScalar(1 + 0.14 * overshoot(hover[SHEET_KEY + i]));
    }
    for (let file = 0; file < files.length; file++)
      fileGroups.current[file]?.scale.setScalar(
        1 + 0.14 * overshoot(hover[FILE_KEY + file]),
      );
    for (let i = 0; i < examples.length; i++) {
      cubeMeshes.current[i]?.scale.setScalar(
        1 + 0.14 * overshoot(hover[CUBE_KEY + i]),
      );
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
      cueStart.current = time;
    }
    // Each stage and each new selection drops the column from the upper board.
    if (droppedFor.current !== cue.id * 100 + selected) {
      droppedFor.current = cue.id * 100 + selected;
      dropStart.current = time;
    }
    const [x, z] = examples[selected].position;
    for (let layer = 0; layer < columns.current.length; layer++) {
      const column = columns.current[layer];
      if (!column) continue;
      const drop = reduced
        ? 1
        : glide(
            progress(time - dropStart.current - layer * 0.15, DROP_DURATION),
          );
      if (drop < 1) moving = true;
      column.position.set(x, heights[layer], z);
      column.scale.y =
        (heights[layer + 1] - heights[layer]) * Math.max(drop, 0.001);
    }
    // Step k starts at k * CUE_STEP and draws its connections along their curves;
    // reduced motion applies the final state at once.
    const elapsed = reduced ? Infinity : (time - cueStart.current) * 1000;
    if (steps && elapsed < (steps.length - 1) * CUE_STEP + TRACE_DURATION)
      moving = true;
    for (let i = 0; i < connections.length; i++) {
      const k = firstStep[i];
      const drawn =
        k < 0 ? 0 : progress(elapsed - k * CUE_STEP, TRACE_DURATION);
      const eased = 1 - (1 - drawn) ** 3;
      curves[i].trace.setDrawRange(0, 2 * Math.round(SEGMENTS * eased));
      const trace = traces.current[i];
      if (trace) trace.visible = drawn > 0;
      // A bright head leads each connection while it draws.
      const pulse = pulses.current[i];
      if (pulse) {
        pulse.visible = drawn > 0 && drawn < 1;
        if (pulse.visible) curves[i].curve.getPoint(eased, pulse.position);
      }
      const line = links.current[2 * i];
      const arrow = links.current[2 * i + 1];
      if (line && easeColor(line.color, steps ? dim : lit, amount))
        moving = true;
      if (
        arrow &&
        easeColor(arrow.color, !steps || drawn >= 1 ? lit : dim, amount)
      )
        moving = true;
    }
    tilt.current.updateWorldMatrix(true, true);
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
      // Close views push boards past the frame; their labels leave with them.
      element.style.visibility =
        shown &&
        projected.z < 1 &&
        Math.abs(projected.x) <= 1 &&
        Math.abs(projected.y) <= 1
          ? "visible"
          : "hidden";
    };
    examples.forEach(({ position: [x, z] }, i) =>
      place(labels.current[i], x, heights[1] + 0.2, z, layers[1], 16),
    );
    files.forEach(({ x }, i) =>
      place(fileLabels.current[i], x, heights[0] + 0.65, 0, layers[0], -30),
    );
    layers.forEach((shown, i) =>
      place(boardLabels.current[i], -2.65, heights[i], 1.35, shown, 8),
    );
    if (firstFrame.current) {
      firstFrame.current = false;
      onReady();
    }
    // Continuous while visible; with reduced motion, only until a change settles.
    if (!reduced || moving) invalidate();
  });
  // Pointer handlers for one hover key; the event stops at the nearest object.
  const hoverProps = (key: number) => ({
    onPointerOver: (event: { stopPropagation: () => void }) => {
      event.stopPropagation();
      onHover(key);
    },
    onPointerOut: () => onHover(-1),
  });
  return (
    <group ref={tilt}>
      <group ref={group} rotation={[0, 0.2, 0]}>
        {layers.map(
          (shown, layer) =>
            shown && (
              <group
                key={layer}
                ref={(element) => {
                  boardGroups.current[layer] = element;
                }}
              >
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
                      onSelect(
                        examples.findIndex((item) => item.file === file),
                      );
                    };
                    return (
                      <group
                        key={file}
                        position={[x, 0.32, 0]}
                        ref={(element) => {
                          fileGroups.current[file] = element;
                        }}
                      >
                        <mesh onClick={select} {...hoverProps(FILE_KEY + file)}>
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
                          <mesh
                            ref={(element) => {
                              cubeMeshes.current[i] = element;
                            }}
                            position={[0, 0.16, 0]}
                            onClick={select}
                            {...hoverProps(CUBE_KEY + i)}
                          >
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
                            {...hoverProps(SHEET_KEY + i)}
                          >
                            {/* The frame loop places each sheet, so they can fan out. */}
                            {[0, 1, 2, 3].map((sheet) => (
                              <mesh
                                key={sheet}
                                ref={(element) => {
                                  sheetMeshes.current[4 * i + sheet] = element;
                                }}
                                onClick={select}
                              >
                                <boxGeometry args={[0.6, 0.032, 0.5]} />
                                <meshStandardMaterial
                                  color={selected === i ? "#b0d897" : "#899c8c"}
                                  roughness={0.7}
                                />
                                {sheet === 3 &&
                                  [-0.14, -0.03, 0.08, 0.19].map((z) => (
                                    <mesh key={z} position={[0, 0.025, z]}>
                                      <boxGeometry
                                        args={[0.42, 0.007, 0.022]}
                                      />
                                      <meshBasicMaterial color="#507047" />
                                    </mesh>
                                  ))}
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
                          visible={false}
                        >
                          <sphereGeometry args={[0.05, 12, 8]} />
                          <meshBasicMaterial color="#f4f1e8" />
                        </mesh>
                        <lineSegments geometry={curves[i].geometry}>
                          <lineBasicMaterial
                            ref={(material) => {
                              links.current[2 * i] = material;
                            }}
                            color="#b0d897"
                          />
                        </lineSegments>
                        <lineSegments
                          ref={(element) => {
                            traces.current[i] = element;
                          }}
                          geometry={curves[i].trace}
                          renderOrder={1}
                          visible={false}
                        >
                          <lineBasicMaterial color="#dcf5c4" />
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
        {/* Unit columns: the frame loop stretches each one between its boards. */}
        {layers.map((shown, layer) =>
          !shown || layer === 2 || !layers[layer + 1] ? null : (
            <group
              key={layer}
              ref={(element) => {
                columns.current[layer] = element;
              }}
            >
              {Array.from({ length: 9 }, (_, i) => (
                <mesh key={i} position={[0, (i + 0.5) / 9, 0]}>
                  <boxGeometry args={[0.018, 1 / 18, 0.018]} />
                  <meshBasicMaterial color="#b0d897" />
                </mesh>
              ))}
            </group>
          ),
        )}
      </group>
    </group>
  );
}
export default function SpatialAtlas({
  selected,
  layers,
  focusLayer,
  view,
  separation,
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
  view?: View;
  separation: number;
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
  // The object under the pointer, or whose label is hovered or focused; -1 for none.
  const [hovered, setHovered] = useState(-1);
  const hoverLabel = (key: number) => ({
    onPointerEnter: () => setHovered(key),
    onPointerLeave: () => setHovered(-1),
    onFocus: () => setHovered(key),
    onBlur: () => setHovered(-1),
  });
  return (
    <>
      <Canvas
        aria-hidden="true"
        tabIndex={-1}
        camera={{ position: START, fov: 43 }}
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
        <CameraControls
          active={active}
          reset={reset}
          reduced={reduced}
          view={view}
          focusLayer={focusLayer}
          separation={separation}
        />
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
          separation={separation}
          cue={cue}
          apart={view !== undefined}
          hovered={hovered}
          onSelect={onSelect}
          onHover={setHovered}
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
            data-hover={
              hovered === CUBE_KEY + i || hovered === SHEET_KEY + i || undefined
            }
            onClick={() => onSelect(i)}
            {...hoverLabel(CUBE_KEY + i)}
          >
            {name}
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
            data-hover={hovered === FILE_KEY + i || undefined}
            onClick={() =>
              onSelect(examples.findIndex(({ file }) => file === i))
            }
            {...hoverLabel(FILE_KEY + i)}
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
