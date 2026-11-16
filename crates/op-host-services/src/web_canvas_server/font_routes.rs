//! Managed-only font routes, after the shared origin/scope gates.
use super::*;

pub(super) fn serve<S: Read + Write>(
    stream: &mut S,
    req: &crate::mcp_serve::HttpRequest,
    ctx: &super::connection::ConnCtx<'_>,
    cors_origin: Option<&str>,
) -> Result<bool> {
    let mut guard = ctx.state.lock().unwrap_or_else(|p| p.into_inner());
    if !matches!(ctx.mode, ServeMode::Managed) {
        crate::mcp_serve::write_mcp_http_response_with_origin(
            stream,
            "404 Not Found",
            "{}",
            cors_origin,
        )?;
    } else if req.method == "POST" && req.path == "/api/mcp/fonts" {
        if guard
            .managed_token
            .as_ref()
            .zip(req.token.as_ref())
            .is_none_or(|(expected, actual)| expected != actual)
        {
            crate::mcp_serve::write_mcp_http_response_with_origin(
                stream,
                "403 Forbidden",
                r#"{"ok":false,"error":"font registration requires supervisor token"}"#,
                cors_origin,
            )?;
        } else {
            let reply = match guard.runtime_fonts.register(&req.body) {
                Ok(()) => guard.runtime_fonts.manifest(),
                Err(e) => {
                    let status = if matches!(e, crate::runtime_fonts::RegisterError::Limit) {
                        "413 Payload Too Large"
                    } else {
                        "400 Bad Request"
                    };
                    crate::mcp_serve::write_mcp_http_response_with_origin(
                        stream,
                        status,
                        &crate::mcp_serve::rest_error_body(&e.to_string()),
                        cors_origin,
                    )?;
                    return Ok(false);
                }
            };
            // Availability is updated only after the real native registry
            // accepted all bytes. No document nodes or versions change.
            guard.editor.editor_ui.imported_font_families = std::sync::Arc::new(
                jian_skia::list_families()
                    .into_iter()
                    .map(|m| m.family)
                    .collect(),
            );
            crate::mcp_serve::write_mcp_http_response_with_origin(
                stream,
                "200 OK",
                &reply,
                cors_origin,
            )?;
        }
    } else if req.method == "POST" && req.path == "/api/mcp/fonts/readiness" {
        let reply = match guard.runtime_fonts.note_browser_readiness(&req.body) {
            Ok(()) => ("200 OK", r#"{"ok":true}"#.to_string()),
            Err(e) => (
                "400 Bad Request",
                crate::mcp_serve::rest_error_body(&e.to_string()),
            ),
        };
        crate::mcp_serve::write_mcp_http_response_with_origin(
            stream,
            reply.0,
            &reply.1,
            cors_origin,
        )?;
    } else if req.method == "GET" && req.path == "/api/mcp/fonts" {
        crate::mcp_serve::write_mcp_http_response_with_origin(
            stream,
            "200 OK",
            &guard.runtime_fonts.manifest(),
            cors_origin,
        )?;
    } else if req.method == "GET" {
        if let Some(bytes) = guard.runtime_fonts.bytes(&req.path) {
            let reply = crate::web_static::StaticReply {
                status: "200 OK",
                content_type: "font/ttf",
                body: bytes.to_vec(),
            };
            crate::web_static::write_static_response(stream, &reply, cors_origin)?;
        } else {
            crate::mcp_serve::write_mcp_http_response_with_origin(
                stream,
                "404 Not Found",
                "{}",
                cors_origin,
            )?;
        }
    } else {
        crate::mcp_serve::write_mcp_http_response_with_origin(
            stream,
            "405 Method Not Allowed",
            "{}",
            cors_origin,
        )?;
    }
    Ok(false)
}

#[cfg(test)]
#[path = "font_route_tests.rs"]
mod tests;
