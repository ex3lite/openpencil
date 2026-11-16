use super::*;
use std::sync::Mutex;

struct Stream {
    input: std::io::Cursor<Vec<u8>>,
    output: Vec<u8>,
}
impl Read for Stream {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        self.input.read(out)
    }
}
impl Write for Stream {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.output.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn serve(
    state: &Mutex<WebCanvasState>,
    method: &str,
    path: &str,
    body: &str,
    headers: &str,
) -> Vec<u8> {
    let request = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:3100\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n{body}",body.len());
    let mut stream = Stream {
        input: std::io::Cursor::new(request.into_bytes()),
        output: Vec::new(),
    };
    connection::serve_one_in_mode(&mut stream, state, &SseHub::default(), ServeMode::Managed)
        .unwrap();
    stream.output
}

#[test]
fn supervisor_registration_requires_token_and_keeps_document_exact() {
    let bytes = include_bytes!("../../../op-host-desktop/assets/fonts/DMMono-Regular.ttf");
    let body = serde_json::json!({"fonts":[{"dataBase64":base64::engine::general_purpose::STANDARD.encode(bytes)}]}).to_string();
    let mut state = WebCanvasState::new(EditorState::default(), 3100);
    state.mode = ServeMode::Managed;
    state.managed_token = Some("test-supervisor-token".into());
    let doc_before = serde_json::to_value(&state.editor.doc).unwrap();
    let state = Mutex::new(state);
    let refused = serve(&state, "POST", "/api/mcp/fonts", &body, "");
    assert!(String::from_utf8_lossy(&refused).contains("403 Forbidden"));
    let foreign = serve(
        &state,
        "POST",
        "/api/mcp/fonts",
        &body,
        "X-OpenPencil-Token: test-supervisor-token\r\nOrigin: https://foreign.invalid\r\n",
    );
    assert!(String::from_utf8_lossy(&foreign).contains("403 Forbidden"));
    let registered = serve(
        &state,
        "POST",
        "/api/mcp/fonts",
        &body,
        "X-OpenPencil-Token: test-supervisor-token\r\n",
    );
    let reply = String::from_utf8(registered).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    let manifest: serde_json::Value =
        serde_json::from_str(reply.split_once("\r\n\r\n").unwrap().1).unwrap();
    let hash = manifest["faces"][0]["sha256"].as_str().unwrap();
    let downloaded = serve(&state, "GET", &format!("/api/mcp/fonts/{hash}"), "", "");
    let split = downloaded
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap()
        + 4;
    assert_eq!(&downloaded[split..], bytes);
    let guard = state.lock().unwrap();
    assert_eq!(guard.version, 0);
    assert_eq!(serde_json::to_value(&guard.editor.doc).unwrap(), doc_before);
}
