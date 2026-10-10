"use client";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import {
  BufferGeometry,
  Group,
  QuadraticBezierCurve3,
  Vector3,
  PerspectiveCamera,
} from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { useTranslation } from "react-i18next";
import { FileCode2 } from "lucide-react";

import {
  connections,
  examples,
  layerHeight,
  layerKeys,
  symbolPositions,
  type AtlasLayers,
} from "./atlas-data";

function linkGeometry(a: number, b: number) {
  const [ax, az] = symbolPositions[a];
  const [bx, bz] = symbolPositions[b];
  const curve = new QuadraticBezierCurve3(
    new Vector3(ax, 0.15, az),
    new Vector3((ax + bx) / 2, 0.45, (az + bz) / 2 - 0.35),
    new Vector3(bx, 0.15, bz),
  );
  const points = curve.getPoints(40);
  return new BufferGeometry().setFromPoints(
    points.slice(0, -1).flatMap((point, i) => [point, points[i + 1]]),
  );
}
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
  separation,
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
  separation: number;
  labels: RefObject<(HTMLButtonElement | null)[]>;
  fileLabels: RefObject<(HTMLButtonElement | null)[]>;
  boardLabels: RefObject<(HTMLSpanElement | null)[]>;
  onSelect: (index: number) => void;
  onReady: () => void;
  onFailure: () => void;
}) {
  const group = useRef<Group>(null);
  const firstFrame = useRef(true);
  const { invalidate, gl } = useThree();
  const projected = useMemo(() => new Vector3(), []);
  const geometry = useMemo(
    () => connections.map(([a, b]) => linkGeometry(a, b)),
    [],
  );
  useEffect(() => () => geometry.forEach((link) => link.dispose()), [geometry]);
  useEffect(() => {
    if (active) invalidate();
  }, [active, rotation, selected, layers, separation, invalidate]);
  useEffect(() => {
    const lost = (event: Event) => {
      event.preventDefault();
      onFailure();
    };
    gl.domElement.addEventListener("webglcontextlost", lost);
    return () => gl.domElement.removeEventListener("webglcontextlost", lost);
  }, [gl, onFailure]);
  useFrame(({ camera, size }, delta) => {
    if (!active || !group.current) return;
    const remaining = rotation - group.current.rotation.y;
    group.current.rotation.y +=
      remaining * (reduced ? 1 : Math.min(1, delta * 9));
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
    symbolPositions.forEach(([x, z], i) => {
      place(labels.current[i], x, 0.2, z, layers[1], 16);
      place(fileLabels.current[i], x, separation + 0.65, z, layers[0], -30);
    });
    layers.forEach((shown, i) =>
      place(
        boardLabels.current[i],
        -2.65,
        layerHeight(i, separation),
        1.35,
        shown,
        8,
      ),
    );
    if (firstFrame.current) {
      firstFrame.current = false;
      onReady();
    }
    if (Math.abs(remaining) > 0.001) invalidate();
  });
  return (
    <group ref={group} rotation={[0, 0.2, 0]}>
      {layers.map(
        (shown, layer) =>
          shown && (
            <group
              key={layer}
              position={[0, layerHeight(layer, separation), 0]}
            >
              <mesh position={[0, -0.09, 0]}>
                <boxGeometry args={[5.5, 0.14, 2.7]} />
                <meshStandardMaterial
                  color={layer === 1 ? "#334f3c" : "#405246"}
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
              {examples.map((_, i) => {
                const [x, z] = symbolPositions[i];
                const color = selected === i ? "#b0d897" : "#899c8c";
                const select = (event: { stopPropagation: () => void }) => {
                  event.stopPropagation();
                  onSelect(i);
                };
                return (
                  <group key={i} position={[x, 0, z]}>
                    {layer === 0 ? (
                      <group position={[0, 0.32, 0]}>
                        <mesh onClick={select}>
                          <boxGeometry args={[0.4, 0.62, 0.11]} />
                          <meshStandardMaterial
                            color={color}
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
                    ) : layer === 1 ? (
                      <mesh position={[0, 0.16, 0]} onClick={select}>
                        <boxGeometry args={[0.32, 0.32, 0.32]} />
                        <meshStandardMaterial
                          color={color}
                          emissive="#60814b"
                          emissiveIntensity={selected === i ? 0.25 : 0}
                          roughness={0.35}
                        />
                      </mesh>
                    ) : (
                      <group>
                        {[0, 1, 2, 3].map((sheet) => (
                          <mesh
                            key={sheet}
                            position={[0, 0.04 + sheet * 0.045, 0]}
                            onClick={select}
                          >
                            <boxGeometry args={[0.72, 0.032, 0.65]} />
                            <meshStandardMaterial
                              color={color}
                              roughness={0.7}
                            />
                          </mesh>
                        ))}
                        {[-0.17, -0.04, 0.09, 0.22].map((z) => (
                          <mesh key={z} position={[0, 0.2, z]}>
                            <boxGeometry args={[0.5, 0.007, 0.022]} />
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
                  const [bx, bz] = symbolPositions[b];
                  const [ax, az] = symbolPositions[a];
                  return (
                    <group key={i}>
                      <lineSegments geometry={geometry[i]}>
                        <lineBasicMaterial color="#b0d897" />
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
                        <meshBasicMaterial color="#b0d897" />
                      </mesh>
                    </group>
                  );
                })}
            </group>
          ),
      )}
      {layers.map((shown, layer) => {
        if (!shown || layer === 2 || !layers[layer + 1]) return null;
        const [x, z] = symbolPositions[selected];
        const top = layerHeight(layer, separation);
        const bottom = layerHeight(layer + 1, separation);
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
  separation,
  active,
  reduced,
  onSelect,
  onReady,
  onFailure,
}: {
  selected: number;
  layers: AtlasLayers;
  separation: number;
  active: boolean;
  reduced: boolean;
  onSelect: (index: number) => void;
  onReady: () => void;
  onFailure: () => void;
}) {
  const { t } = useTranslation();
  const [rotation, setRotation] = useState(0.2);
  const [reset, setReset] = useState(0);
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
          separation={separation}
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
            {name}
          </button>
        ))}
      </div>
      <div className="spatial-file-labels">
        {examples.map((item, i) => (
          <button
            key={item.filename}
            type="button"
            className="spatial-label spatial-file-label"
            ref={(element) => {
              fileLabels.current[i] = element;
            }}
            aria-label={`${t("inspectFile")} src/${item.filename}`}
            aria-pressed={selected === i}
            onClick={() => onSelect(i)}
          >
            <FileCode2 size={12} aria-hidden="true" /> {item.filename}
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
      <div className="spatial-controls">
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
      </div>
    </>
  );
}
