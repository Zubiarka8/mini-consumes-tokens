import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import type { Bridge, Source } from "./types";

export function useMedia(query: string): boolean {
  const subscribe = useCallback(
    (notify: () => void) => {
      const list = window.matchMedia(query);
      list.addEventListener("change", notify);
      return () => list.removeEventListener("change", notify);
    },
    [query],
  );
  return useSyncExternalStore(subscribe, () => window.matchMedia(query).matches);
}

/** The dashboard's theme attribute, which the shell changes in place. */
export function useTheme(): "light" | "dark" {
  const read = (): "light" | "dark" => (document.documentElement.dataset.theme === "dark" ? "dark" : "light");
  const [theme, setTheme] = useState(read);
  useEffect(() => {
    const observer = new MutationObserver(() => setTheme(read()));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => observer.disconnect();
  }, []);
  return theme;
}

export type Loaded<T> =
  | { status: "loading" }
  | { status: "ready"; data: T }
  | { status: "failed"; message: string; retry(): void };

/**
 * One source fragment, or none when `params` is null. A newer request with
 * the same `key` aborts the older one, and leaving cancels it.
 */
export function useSource(
  bridge: Bridge,
  params: { path: string; start: number; end: number } | null,
  key: string,
): Loaded<Source> | null {
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<Loaded<Source>>({ status: "loading" });
  const wanted = params ? `${params.path}:${params.start}:${params.end}` : null;
  useEffect(() => {
    if (!params) return;
    let live = true;
    setState({ status: "loading" });
    bridge.source(params, key).then(
      (data) => live && setState({ status: "ready", data }),
      (error: Error) => {
        if (!live || error?.name === "AbortError") return;
        setState({ status: "failed", message: error.message, retry: () => setAttempt((n) => n + 1) });
      },
    );
    return () => {
      live = false;
      bridge.cancel(key);
    };
    // `wanted` names the request; `params` is a new object on every render.
  }, [wanted, attempt, key, bridge]);
  return params ? state : null;
}

/** Best-effort storage for panel sizes: private windows may block it. */
export const layoutStorage = {
  getItem(name: string) {
    try {
      return localStorage.getItem(`mct:${name}`);
    } catch {
      return null;
    }
  },
  setItem(name: string, value: string) {
    try {
      localStorage.setItem(`mct:${name}`, value);
    } catch {
      /* the default sizes are used next time */
    }
  },
};
