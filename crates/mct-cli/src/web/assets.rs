//! The dashboard front end, embedded in the binary and served from the same
//! origin so the page can keep a strict `script-src 'self'` policy. The shell
//! and its pages are plain ES modules with no build step; the open finding's
//! review workspace is a React bundle that `crates/mct-cli/web-ui` compiles
//! into `review-dist/`, committed so that building the binary needs no Node.

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
        "js/i18n-en.js" => (JS, app!("i18n-en.js")),
        "js/issues.js" => (JS, app!("issues.js")),
        "js/components.js" => (JS, app!("components.js")),
        "js/charts.js" => (JS, app!("charts.js")),
        "js/ai.js" => (JS, app!("ai.js")),
        "js/code.js" => (JS, app!("code.js")),
        "js/graph.js" => (JS, app!("graph.js")),
        "js/markdown.js" => (JS, app!("markdown.js")),
        "js/palette.js" => (JS, app!("palette.js")),
        "js/ignore-card.js" => (JS, app!("ignore-card.js")),
        "js/page-dashboard.js" => (JS, app!("page-dashboard.js")),
        "js/page-symbols.js" => (JS, app!("page-symbols.js")),
        "js/page-findings.js" => (JS, app!("page-findings.js")),
        "js/page-search.js" => (JS, app!("page-search.js")),
        "js/page-deps.js" => (JS, app!("page-deps.js")),
        "js/page-stats.js" => (JS, app!("page-stats.js")),
        "js/page-notes.js" => (JS, app!("page-notes.js")),
        "js/page-workspace.js" => (JS, app!("page-workspace.js")),
        "js/page-settings.js" => (JS, app!("page-settings.js")),
        // Loaded only when a finding is opened (page-findings.js).
        "js/review.js" => (JS, include_str!("review-dist/review.js")),
        "review.css" => (CSS, include_str!("review-dist/review.css")),
        _ => return None,
    };
    Some(Response::ok(kind, body.to_string()))
}
