// How much the open dashboard may do and for how long the server stays up, both set in Settings.
// A value is minutes; 0 means never. They follow the user to other projects like the theme does
// (see prefs.js), because every project's dashboard has its own port.
import { sharedPref } from "./prefs.js";

export const MINUTES = [5, 10, 15, 20, 25, 30, 60, 120, 180];
export const MAX_MINUTES = 24 * 60;
const DEFAULTS = { stop: 30, pause: 5 };
const USE_PING_MS = 60_000;

const valid = (value) => Number.isInteger(value) && value >= 0 && value <= MAX_MINUTES;

export const power = {
  /** `stop`: minutes without use before the server closes. `pause`: minutes without input before
   *  this page stops checking for a new index. */
  get(name) {
    let raw = null;
    try {
      raw = localStorage.getItem(`mct:power-${name}`);
    } catch {
      /* fall through to the shared copy */
    }
    const value = Number(raw ?? sharedPref.get(`power-${name}`) ?? NaN);
    return valid(value) ? value : DEFAULTS[name];
  },
  set(name, minutes) {
    if (!valid(minutes)) return;
    sharedPref.set(`power-${name}`, String(minutes));
    try {
      localStorage.setItem(`mct:power-${name}`, String(minutes));
    } catch {
      /* ignore */
    }
  },
};

let lastInput = Date.now();

/** True once the person has not touched the page for the `pause` time. */
export function livePaused() {
  const minutes = power.get("pause");
  return minutes > 0 && Date.now() - lastInput > minutes * 60_000;
}

/** Calls `onUse` at most once a minute while the person works with the page (keys, pointer,
 *  scroll, touch). Passive listeners: they cost nothing between inputs. */
export function trackUse(onUse) {
  let last = 0;
  const seen = () => {
    const now = Date.now();
    lastInput = now;
    if (now - last < USE_PING_MS) return;
    last = now;
    onUse();
  };
  for (const name of ["keydown", "pointerdown", "wheel", "touchstart", "scroll"])
    addEventListener(name, seen, { passive: true, capture: true });
}
