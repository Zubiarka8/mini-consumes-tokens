// Preferences that follow the user from one project to the next. localStorage belongs to one origin
// and every project has its own port, so a choice made on one dashboard would not reach another.
// Cookies ignore the port on localhost: the choice is mirrored in one small cookie, read when this
// origin has nothing saved yet. Everything is best effort (storage and cookies may be blocked).
const COOKIE = "mct-prefs";
const YEAR = 365 * 24 * 60 * 60;

function readAll() {
  try {
    const entry = document.cookie.split("; ").find((part) => part.startsWith(`${COOKIE}=`));
    const parsed = entry ? JSON.parse(decodeURIComponent(entry.slice(COOKIE.length + 1))) : {};
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

export const sharedPref = {
  /** The shared string saved under `name`, or null. */
  get(name) {
    const value = readAll()[name];
    return typeof value === "string" ? value : null;
  },
  set(name, value) {
    try {
      const all = encodeURIComponent(JSON.stringify({ ...readAll(), [name]: value }));
      document.cookie = `${COOKIE}=${all}; Max-Age=${YEAR}; Path=/; SameSite=Strict`;
    } catch {
      /* cookies blocked: the choice stays in localStorage */
    }
  },
};
