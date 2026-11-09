"use client";
import { useEffect } from "react";

// Sections and cards fade in once as they enter the viewport. The hidden
// state is set from here, so the page stays fully visible without JavaScript.
export function RevealOnScroll() {
  useEffect(() => {
    const targets = [
      ...document.querySelectorAll<HTMLElement>(
        ".landing main > section:not(.hero), .landing main h2, .landing .feature-grid article, .landing .cap-list article, .landing .faq details, .landing .command-row",
      ),
    ];
    if (typeof IntersectionObserver === "undefined") return;
    const owner = new Map<Element, HTMLElement>();
    const seen = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (!e.isIntersecting) continue;
          (owner.get(e.target) ?? e.target).classList.add("in");
          seen.unobserve(e.target);
        }
      },
      { rootMargin: "0px 0px -8% 0px", threshold: 0.08 },
    );
    targets.forEach((el, i) => {
      if (el.getBoundingClientRect().top < innerHeight * 0.9) return;
      el.setAttribute("data-reveal", el.tagName === "H2" ? "wipe" : "");
      el.style.setProperty("--d", `${(i % 4) * 70}ms`);
      // A clipped heading has no visible area, so watch its parent instead.
      const watched = el.tagName === "H2" ? (el.parentElement ?? el) : el;
      owner.set(watched, el);
      seen.observe(watched);
    });
    // Scroll-linked motion: parallax for the hero map, windows that settle
    // into place and a header that gains a surface once the page moves.
    const root = document.querySelector<HTMLElement>(".landing");
    const calm = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const windows = [
      ...document.querySelectorAll<HTMLElement>(
        ".landing .mock-window, .landing .docs-art",
      ),
    ];
    let raf = 0;
    const frame = () => {
      raf = 0;
      if (!root) return;
      root.style.setProperty("--sy", String(Math.round(scrollY)));
      root.style.setProperty(
        "--progress",
        (
          scrollY /
          Math.max(1, document.documentElement.scrollHeight - innerHeight)
        ).toFixed(4),
      );
      root.toggleAttribute("data-scrolled", scrollY > 24);
      for (const el of windows) {
        const top = el.getBoundingClientRect().top;
        const t = Math.min(
          1,
          Math.max(0, (innerHeight * 0.95 - top) / (innerHeight * 0.55)),
        );
        el.style.setProperty("--vis", t.toFixed(3));
      }
    };
    const onScroll = () => {
      if (!raf) raf = requestAnimationFrame(frame);
    };
    if (!calm) {
      frame();
      addEventListener("scroll", onScroll, { passive: true });
      addEventListener("resize", onScroll);
    }
    return () => {
      seen.disconnect();
      cancelAnimationFrame(raf);
      removeEventListener("scroll", onScroll);
      removeEventListener("resize", onScroll);
    };
  }, []);
  return null;
}
