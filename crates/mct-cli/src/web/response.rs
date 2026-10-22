//! The one response shape shared by the HTTP layer and the API/static routes.

pub(super) struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Response {
    pub fn ok(content_type: &'static str, body: String) -> Self {
        Self {
            status: 200,
            content_type,
            body,
        }
    }

    pub fn json(value: &serde_json::Value) -> Self {
        Self::ok("application/json; charset=utf-8", value.to_string())
    }

    pub fn error(status: u16) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: format!("Request rejected ({status}).\n"),
        }
    }

    pub fn failure(status: u16, message: &str) -> Self {
        Self::json(&serde_json::json!({ "error": message })).with_status(status)
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
}
