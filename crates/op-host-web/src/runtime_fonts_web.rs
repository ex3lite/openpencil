//! Permission-free font transport from the managed daemon to CanvasKit.
//! Polls only the manifest when unchanged, with at most four face downloads.

use crate::repaint_ctx::RepaintContext;
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use wasm_bindgen::{closure::Closure, JsCast};

#[derive(Deserialize)]
struct Manifest {
    generation: u64,
    faces: Vec<Face>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Face {
    sha256: String,
    byte_length: usize,
}

#[derive(Default)]
struct LoadState {
    loaded: BTreeSet<String>,
    pending: BTreeSet<String>,
    generation: u64,
}

pub(crate) fn start<C: RepaintContext + 'static>(inner: &Rc<RefCell<C>>) {
    let weak = Rc::downgrade(inner);
    let loads = Rc::new(RefCell::new(LoadState::default()));
    let in_flight = Rc::new(Cell::new(false));
    let interval = Rc::new(Cell::new(0));
    let interval_cb = interval.clone();
    let callback = Closure::<dyn FnMut()>::new(move || {
        let Some(inner) = weak.upgrade() else {
            if let Some(window) = web_sys::window() {
                window.clear_interval_with_handle(interval_cb.get());
            }
            return;
        };
        if in_flight.replace(true) {
            return;
        }
        let in_flight_cb = in_flight.clone();
        let loads_cb = loads.clone();
        let started = crate::live_sync::get_with_status(
            &crate::daemon_base::daemon_url("/api/mcp/fonts"),
            Rc::new(move |status, body| {
                in_flight_cb.set(false);
                if status != 200 {
                    return;
                }
                let Ok(manifest) = serde_json::from_str::<Manifest>(&body) else {
                    return;
                };
                if manifest.faces.len() > 512 {
                    return;
                }
                loads_cb.borrow_mut().generation = manifest.generation;
                for face in manifest.faces {
                    let valid_hash = face.sha256.len() == 64
                        && face
                            .sha256
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
                    let mut loads = loads_cb.borrow_mut();
                    if !valid_hash
                        || face.byte_length > op_editor_core::runtime_font::MAX_FONT_BYTES
                        || loads.loaded.contains(&face.sha256)
                        || loads.pending.contains(&face.sha256)
                        || loads.pending.len() >= 4
                    {
                        continue;
                    }
                    loads.pending.insert(face.sha256.clone());
                    drop(loads);
                    load_face(&inner, &loads_cb, face);
                }
                report_readiness(&loads_cb);
            }),
        );
        if !started {
            in_flight.set(false);
        }
    });
    if let Some(window) = web_sys::window() {
        if let Ok(id) = window.set_interval_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            1500,
        ) {
            interval.set(id);
            callback.forget();
        }
    }
}

fn load_face<C: RepaintContext + 'static>(
    inner: &Rc<RefCell<C>>,
    loads: &Rc<RefCell<LoadState>>,
    face: Face,
) {
    let inner = inner.clone();
    let loads = loads.clone();
    let route = format!("/api/mcp/fonts/{}", face.sha256);
    crate::web_asset_fetch::fetch_bytes(&crate::daemon_base::daemon_url(&route), move |result| {
        loads.borrow_mut().pending.remove(&face.sha256);
        let Ok(bytes) = result else {
            return;
        };
        if bytes.len() != face.byte_length {
            return;
        }
        // Parse independently; do not trust a manifest alias or axis offset.
        let Ok(meta) = op_editor_core::runtime_font::parse(&bytes) else {
            return;
        };
        let Ok(mut b) = inner.try_borrow_mut() else {
            return;
        };
        let mut value = serde_json::to_value(&meta).expect("font metadata serializes");
        value["sha256"] = face.sha256.clone().into();
        if !b.register_runtime_font(&bytes, &value.to_string()) {
            return;
        }
        loads.borrow_mut().loaded.insert(face.sha256);
        let mut families = (*b.host().editor_state().editor_ui.imported_font_families).clone();
        if !families.contains(&meta.family) {
            families.push(meta.family);
        }
        b.host_mut()
            .editor_state_mut()
            .editor_ui
            .imported_font_families = std::sync::Arc::new(families);
        b.host_mut().mark_editor_state_dirty();
        b.host_mut().invalidate_layout_scene();
        b.host_mut().refresh_missing_fonts_prompt();
        let _ = b.repaint();
        report_readiness(&loads);
    });
}

fn report_readiness(loads: &Rc<RefCell<LoadState>>) {
    let loads = loads.borrow();
    let body =
        serde_json::json!({"generation":loads.generation,"loadedSha256":loads.loaded}).to_string();
    crate::live_sync::post_json(
        &crate::daemon_base::daemon_url("/api/mcp/fonts/readiness"),
        &body,
        None,
    );
}
