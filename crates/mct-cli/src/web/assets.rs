//! The dashboard front end, embedded in the binary. There is no build step and
//! no third-party code: plain ES modules and one stylesheet, served from the
//! same origin so the page can keep a strict `script-src 'self'` policy.

use super::response::Response;

const HTML: &str = "text/html; charset=utf-8";
const CSS: &str = "text/css; charset=utf-8";
const JS: &str = "text/javascript; charset=utf-8";

macro_rules! app {
    ($file:literal) => {
        include_str!(concat!("app/", $file))
    };
}

pub(super) fn serve(path: &str) -> Option<Response> {
    let (kind, body) = match path {
        "" | "index.html" => (HTML, app!("index.html")),
        "app.css" => (CSS, app!("app.css")),
        "js/main.js" => (JS, app!("main.js")),
        "js/core.js" => (JS, app!("core.js")),
        "js/i18n.js" => (JS, app!("i18n.js")),
        "js/components.js" => (JS, app!("components.js")),
        "js/charts.js" => (JS, app!("charts.js")),
        "js/ai.js" => (JS, app!("ai.js")),
        "js/code.js" => (JS, app!("code.js")),
        "js/graph.js" => (JS, app!("graph.js")),
        "js/markdown.js" => (JS, app!("markdown.js")),
        "js/palette.js" => (JS, app!("palette.js")),
        "js/page-dashboard.js" => (JS, app!("page-dashboard.js")),
        "js/page-symbols.js" => (JS, app!("page-symbols.js")),
        "js/page-findings.js" => (JS, app!("page-findings.js")),
        "js/page-search.js" => (JS, app!("page-search.js")),
        "js/page-deps.js" => (JS, app!("page-deps.js")),
        "js/page-stats.js" => (JS, app!("page-stats.js")),
        "js/page-notes.js" => (JS, app!("page-notes.js")),
        "js/page-workspace.js" => (JS, app!("page-workspace.js")),
        "js/page-settings.js" => (JS, app!("page-settings.js")),
        _ => return None,
    };
    Some(Response::ok(kind, body.to_string()))
}
