"use client";
import { useEffect } from "react";

// Sections and cards fade in once as they enter the viewport. The hidden
// state is set from here, so the page stays fully visible without JavaScript.
export function RevealOnScroll() {
  useEffect(() => {
    const targets = [
      ...document.querySelectorAll<HTMLElement>(
        ".landing main > section, .landing .feature-grid article, .landing .cap-list article, .landing .faq details, .landing .command-row",
      ),
    ];
    if (typeof IntersectionObserver === "undefined") return;
    const seen = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (!e.isIntersecting) continue;
          e.target.classList.add("in");
          seen.unobserve(e.target);
        }
      },
      { rootMargin: "0px 0px -8% 0px", threshold: 0.08 },
    );
    targets.forEach((el, i) => {
      if (el.getBoundingClientRect().top < innerHeight * 0.9) return;
      el.setAttribute("data-reveal", "");
      el.style.setProperty("--d", `${(i % 4) * 70}ms`);
      seen.observe(el);
    });
    return () => seen.disconnect();
  }, []);
  return null;
}
