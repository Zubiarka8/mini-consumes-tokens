use std::borrow::Cow;
use std::sync::Mutex;

use rmcp::ErrorData as McpError;
use rmcp::model::{ClientCapabilities, ClientRequest, ProtocolVersion, ServerResult};
use rmcp::service::{NotificationContext, RequestContext, RoleServer, Service, ServiceRole};

/// Works around rmcp (<= 3.4.1) permanently flagging a stdio session as
/// stateless when the client's first message is `server/discover`: if the
/// client then falls back to a legacy `initialize` handshake (as Claude Code
/// does), every later request without per-request `_meta` is rejected with
/// "request _meta is missing or has malformed required fields". After a
/// successful `initialize`, this fills those keys from the negotiated session.
pub struct SessionCompat<S> {
    inner: S,
    negotiated: Mutex<Option<(ProtocolVersion, ClientCapabilities)>>,
}

impl<S> SessionCompat<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            negotiated: Mutex::new(None),
        }
    }
}

impl<S: Service<RoleServer>> Service<RoleServer> for SessionCompat<S> {
    async fn handle_request(
        &self,
        request: <RoleServer as ServiceRole>::PeerReq,
        mut context: RequestContext<RoleServer>,
    ) -> Result<<RoleServer as ServiceRole>::Resp, McpError> {
        let init_capabilities = match &request {
            ClientRequest::InitializeRequest(init) => Some(init.params.capabilities.clone()),
            _ => None,
        };

        if init_capabilities.is_none() {
            let negotiated = self.negotiated.lock().ok().and_then(|guard| guard.clone());
            if let Some((version, capabilities)) = negotiated {
                if context.meta.protocol_version().is_none() {
                    context.meta.set_protocol_version(version);
                }
                if context.meta.client_capabilities().is_none() {
                    context.meta.set_client_capabilities(capabilities);
                }
            }
        }

        let result = self.inner.handle_request(request, context).await;

        if let (Some(capabilities), Ok(ServerResult::InitializeResult(init))) =
            (init_capabilities, &result)
        {
            if let Ok(mut guard) = self.negotiated.lock() {
                *guard = Some((init.protocol_version.clone(), capabilities));
            }
        }
        result
    }

    async fn handle_notification(
        &self,
        notification: <RoleServer as ServiceRole>::PeerNot,
        context: NotificationContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.inner.handle_notification(notification, context).await
    }

    fn get_info(&self) -> <RoleServer as ServiceRole>::Info {
        self.inner.get_info()
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        self.inner.supported_protocol_versions()
    }
}
