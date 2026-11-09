"use client";
import { useEffect, useRef } from "react";

type Node = { x: number; y: number; vx: number; vy: number; r: number };

// A drifting map of "symbols". Links appear between near nodes and the
// cursor pulls a small neighborhood into focus, like a query would.
export function Constellation() {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    let w = 0;
    let h = 0;
    let nodes: Node[] = [];
    let raf = 0;
    let visible = true;
    const mouse = { x: -999, y: -999 };
    const resize = () => {
      const dpr = Math.min(devicePixelRatio || 1, 2);
      w = canvas.clientWidth;
      h = canvas.clientHeight;
      canvas.width = w * dpr;
      canvas.height = h * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const count = Math.round(Math.min(90, (w * h) / 14000));
      nodes = Array.from({ length: count }, () => ({
        x: Math.random() * w,
        y: Math.random() * h,
        vx: (Math.random() - 0.5) * 0.25,
        vy: (Math.random() - 0.5) * 0.25,
        r: 1 + Math.random() * 1.8,
      }));
    };
    const draw = () => {
      const dark = document.documentElement.classList.contains("dark");
      const hot = dark ? "214,242,139" : "59,107,34";
      const cold = dark ? "150,180,160" : "60,90,70";
      const dim = dark ? "rgba(170,200,180,0.5)" : "rgba(40,70,50,0.4)";
      ctx.clearRect(0, 0, w, h);
      for (const n of nodes) {
        if (!still) {
          n.x += n.vx;
          n.y += n.vy;
          if (n.x < 0 || n.x > w) n.vx *= -1;
          if (n.y < 0 || n.y > h) n.vy *= -1;
        }
      }
      for (let i = 0; i < nodes.length; i++) {
        const a = nodes[i];
        const da = Math.hypot(a.x - mouse.x, a.y - mouse.y);
        for (let j = i + 1; j < nodes.length; j++) {
          const b = nodes[j];
          const d = Math.hypot(a.x - b.x, a.y - b.y);
          if (d > 130) continue;
          const near = Math.min(da, Math.hypot(b.x - mouse.x, b.y - mouse.y));
          const glow = near < 170 ? 1 - near / 170 : 0;
          ctx.strokeStyle = `rgba(${glow ? hot : cold},${(1 - d / 130) * (0.16 + glow * 0.7)})`;
          ctx.lineWidth = 0.6 + glow;
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.stroke();
        }
        const lit = da < 170 ? 1 - da / 170 : 0;
        ctx.fillStyle = lit
          ? `rgba(${hot},${0.4 + lit * 0.6})`
          : dim;
        ctx.beginPath();
        ctx.arc(a.x, a.y, a.r + lit * 2.2, 0, 6.283);
        ctx.fill();
      }
      raf = visible && !still ? requestAnimationFrame(draw) : 0;
    };
    const move = (e: PointerEvent) => {
      const r = canvas.getBoundingClientRect();
      mouse.x = e.clientX - r.left;
      mouse.y = e.clientY - r.top;
    };
    const io = new IntersectionObserver(([entry]) => {
      visible = entry.isIntersecting;
      if (visible && !raf) raf = requestAnimationFrame(draw);
    });
    resize();
    io.observe(canvas);
    addEventListener("resize", resize);
    addEventListener("pointermove", move);
    raf = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(raf);
      io.disconnect();
      removeEventListener("resize", resize);
      removeEventListener("pointermove", move);
    };
  }, []);
  return <canvas ref={ref} className="constellation" aria-hidden="true" />;
}
