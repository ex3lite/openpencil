//! Shared document transport budgets for browser sync and native HTTP ingress.
//!
//! This supports complete multi-page design systems with embedded assets. Keep
//! the JSON envelope separate from the document budget so a document accepted by
//! the browser serializer is also accepted by the native HTTP reader.
pub const MAX_DOCUMENT_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_DOCUMENT_REQUEST_BYTES: usize = MAX_DOCUMENT_BYTES + 256 * 1024;

/// Only document-bearing routes get the larger budget. Other integrations keep
/// their own smaller limits (DOM snapshots, credentials, design evidence).
pub fn document_request_body_limit(path: &str) -> usize {
    match path {
        "/api/mcp/document" | "/api/file/save" | "/api/export/raster" | "/api/export/pdf" => {
            MAX_DOCUMENT_REQUEST_BYTES
        }
        _ => 64 * 1024 * 1024,
    }
}
