#![cfg(feature = "canvaskit")]

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn real_canvaskit_preserves_static_weights_and_instances_variable_weight() {
    let crate_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let regular = crate_dir.join("../op-host-desktop/assets/fonts/DMMono-Regular.ttf");
    let medium = crate_dir.join("../op-host-desktop/assets/fonts/DMMono-Medium.ttf");
    let inter = crate_dir.join("../op-host-desktop/assets/fonts/Inter-VF.ttf");
    let mut source = String::from("import { createRequire } from 'module';\nimport fs from 'fs';\nconst require = createRequire(import.meta.url);\n");
    source
        .push_str(&std::fs::read_to_string(crate_dir.join("src/op_ck_runtime_fonts.js")).unwrap());
    source.push_str(&format!(r#"
const CK = await require({ck_js:?})({{wasmBinary:fs.readFileSync({ck_wasm:?})}});
const registry = runtimeFontFactory()(CK);
const assert = (value, message) => {{ if (!value) throw new Error(message); }};
function install(path, meta) {{
  assert(registry.register(new Uint8Array(fs.readFileSync(path)), JSON.stringify(meta)), 'real FreeType accepts ' + meta.family);
}}
install({regular:?}, {regular_meta});
install({medium:?}, {medium_meta});
const russian = 'АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя';
assert(registry.fallback(russian,400,false) === null, 'Latin subset never pretends Cyrillic coverage');
install({inter:?}, {inter_meta});
assert(registry.fallback(russian,400,false) !== null, 'full font supplies all66 Cyrillic glyphs');
const regular = registry.resolve('dm mono',400,false);
const medium = registry.resolve('dm mono',500,false);
assert(regular.key !== medium.key, 'family registry preserves two intrinsic static weights');
assert(regular.actualWeight === 400 && medium.actualWeight === 500, 'correct face metadata');
const a = registry.resolve('inter',400,false), b = registry.resolve('inter',700,false);
assert(a.key !== b.key, 'variable weights use distinct requested variation settings');
function width(entry) {{ return entry.measure('Hamburgefontsiv',32); }}
assert(Math.abs(width(a)-width(b)) > 0.2, 'variable 400/700 have physically different advances');
assert(registry.resolve('nonexistent',400,false) === null, 'no silent family aliases');

"#,
        ck_js=crate_dir.join("assets/canvaskit/canvaskit.js"),
        ck_wasm=crate_dir.join("assets/canvaskit/canvaskit.wasm"),
        regular_meta=metadata(&regular,"regular"), medium_meta=metadata(&medium,"medium"),
        inter_meta=metadata(&inter,"inter")));
    let mut child = Command::new("node")
        .args(["--input-type=module"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("node is required for actual CanvasKit font regression");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "CanvasKit fonts: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn metadata(path: &std::path::Path, hash: &str) -> String {
    let bytes = std::fs::read(path).unwrap();
    let meta = op_editor_core::runtime_font::parse(&bytes).unwrap();
    let mut value = serde_json::to_value(meta).unwrap();
    value["sha256"] = hash.into();
    value.to_string()
}
