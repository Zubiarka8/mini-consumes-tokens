"use client";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { BufferGeometry, Float32BufferAttribute, Group, Vector3 } from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { useTranslation } from "react-i18next";

const nodes: [number, number, number][] = [
  [-0.5, 0.9, 0.4],
  [1.3, -0.3, 0.6],
  [-1.3, -1, -0.4],
  [-2, 1, -0.8],
  [2, 1, -0.6],
  [1.6, -1.5, -0.7],
  [0.4, 1.8, -1.1],
  [-2, -1.6, 0.6],
];
const edges = [
  [0, 1],
  [0, 2],
  [0, 3],
  [0, 6],
  [1, 2],
  [1, 4],
  [1, 5],
  [2, 3],
  [2, 7],
];
function lineGeometry(connections: number[][]) {
  const geometry = new BufferGeometry();
  geometry.setAttribute(
    "position",
    new Float32BufferAttribute(
      connections.flatMap(([a, b]) => [...nodes[a], ...nodes[b]]),
      3,
    ),
  );
  return geometry;
}
function CameraControls({ active, reset }: { active: boolean; reset: number }) {
  const { camera, gl, invalidate } = useThree();
  const controls = useRef<OrbitControls | null>(null);
  useEffect(() => {
    const orbit = new OrbitControls(camera, gl.domElement);
    orbit.enablePan = false;
    orbit.minDistance = 6;
    orbit.maxDistance = 10;
    orbit.minPolarAngle = Math.PI / 4;
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
  active,
  labels,
  onSelect,
  onReady,
  onFailure,
}: {
  selected: number;
  rotation: number;
  active: boolean;
  labels: RefObject<(HTMLButtonElement | null)[]>;
  onSelect: (index: number) => void;
  onReady: () => void;
  onFailure: () => void;
}) {
  const group = useRef<Group>(null);
  const firstFrame = useRef(true);
  const { invalidate, gl } = useThree();
  const projected = useMemo(() => new Vector3(), []);
  useEffect(() => {
    if (active) invalidate();
  }, [active, rotation, selected, invalidate]);
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
    group.current.rotation.y += remaining * Math.min(1, delta * 9);
    group.current.updateWorldMatrix(true, true);
    camera.updateMatrixWorld();
    nodes.slice(0, 3).forEach((position, i) => {
      const label = labels.current[i];
      if (!label) return;
      projected.set(...position);
      group.current!.localToWorld(projected).project(camera);
      label.style.left = `${((projected.x + 1) * size.width) / 2}px`;
      label.style.top = `${((1 - projected.y) * size.height) / 2 + 22}px`;
      label.style.visibility = projected.z < 1 ? "visible" : "hidden";
    });
    if (firstFrame.current) {
      firstFrame.current = false;
      onReady();
    }
    if (Math.abs(remaining) > 0.001) invalidate();
  });
  const geometry = useMemo(() => lineGeometry(edges), []);
  const highlight = useMemo(
    () =>
      lineGeometry(edges.filter(([a, b]) => a === selected || b === selected)),
    [selected],
  );
  useEffect(() => () => geometry.dispose(), [geometry]);
  useEffect(() => () => highlight.dispose(), [highlight]);
  return (
    <group ref={group} rotation={[0.12, 0.2, -0.12]}>
      <lineSegments geometry={geometry}>
        <lineBasicMaterial color="#607b68" transparent opacity={0.6} />
      </lineSegments>
      <lineSegments geometry={highlight}>
        <lineBasicMaterial color="#b0d897" />
      </lineSegments>
      {nodes.map((position, i) =>
        i < 3 ? (
          <mesh
            key={i}
            position={position}
            onClick={(event) => {
              event.stopPropagation();
              onSelect(i);
            }}
          >
            <sphereGeometry args={[i === selected ? 0.2 : 0.12, 32, 24]} />
            <meshStandardMaterial
              color={i === selected ? "#b0d897" : "#587861"}
              emissive="#46633b"
              emissiveIntensity={i === selected ? 0.35 : 0.08}
              roughness={0.35}
              metalness={0.1}
            />
          </mesh>
        ) : (
          <group key={i} position={position}>
            <mesh>
              <boxGeometry args={[0.16, 0.22, 0.06]} />
              <meshStandardMaterial color="#54725e" roughness={0.65} />
            </mesh>
            <mesh position={[0, 0, 0.035]}>
              <planeGeometry args={[0.1, 0.025]} />
              <meshBasicMaterial color="#b0c5b2" />
            </mesh>
          </group>
        ),
      )}
      {[0, 1].map((i) => (
        <mesh key={i} rotation={[Math.PI / 2 + i * 0.6, i * 0.5, 0]}>
          <torusGeometry args={[2.6, 0.005, 6, 128]} />
          <meshBasicMaterial color="#47654f" transparent opacity={0.45} />
        </mesh>
      ))}
    </group>
  );
}
export default function SpatialAtlas({
  selected,
  symbols,
  active,
  onSelect,
  onReady,
  onFailure,
}: {
  selected: number;
  symbols: string[];
  active: boolean;
  onSelect: (index: number) => void;
  onReady: () => void;
  onFailure: () => void;
}) {
  const { t } = useTranslation();
  const [rotation, setRotation] = useState(0.2);
  const [reset, setReset] = useState(0);
  const labels = useRef<(HTMLButtonElement | null)[]>([]);
  return (
    <>
      <Canvas
        aria-hidden="true"
        tabIndex={-1}
        camera={{ position: [0, 0, 7.4], fov: 48 }}
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
          active={active}
          labels={labels}
          onSelect={onSelect}
          onReady={onReady}
          onFailure={onFailure}
        />
      </Canvas>
      <div className="spatial-labels" role="group" aria-label={t("select")}>
        {symbols.map((name, i) => (
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
