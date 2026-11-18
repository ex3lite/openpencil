//! Active-page viewport fitting shared by page-navigation entry points.

use super::WidgetHost;

impl WidgetHost {
    /// Restore a comment's page and selection only when all targets still exist.
    pub(crate) fn reveal_codex_layers(
        &mut self,
        page_id: &str,
        target_ids: &[String],
        w: f32,
        h: f32,
    ) -> bool {
        use op_editor_core::{walkers::find_node, NodeId};
        let state = &mut self.editor_state;
        let (index, children) = match state.doc.pages.as_deref() {
            Some(pages) if !pages.is_empty() => {
                let Some((i, page)) = pages.iter().enumerate().find(|(_, p)| p.id == page_id)
                else {
                    return false;
                };
                (i, &page.children)
            }
            _ if matches!(page_id, "0" | "" | "page-1") => (0, &state.doc.children),
            _ => return false,
        };
        let ids: Option<Vec<_>> = target_ids.iter().cloned().map(NodeId::new_opt).collect();
        let Some(ids) = ids else { return false };
        if ids.is_empty() || ids.iter().any(|id| find_node(children, id).is_none()) {
            return false;
        }
        let changed_page = index != state.ui.active_page_index;
        state.set_active_page(index);
        state.set_single_selection(ids[0].clone());
        for id in &ids[1..] {
            state.toggle_selection(id.clone());
        }
        state.sync_entered_container_with_selection();
        state.editor_ui.last_revealed_layer_anchor = None;
        if changed_page || state.editor_ui.preview.mode {
            self.fit_active_page_after_switch(w, h);
        }
        self.mark_dirty();
        self.refresh_layout_scene();
        op_editor_ui::widgets::host_overlay_geometry::zoom_to_fit_nodes(
            &mut self.editor_state,
            &self.layout_scene,
            target_ids.iter().map(String::as_str),
            w,
            h,
        );
        self.mark_dirty();
        true
    }

    /// Rebuild the newly-active page scene before deriving its fit. Page
    /// navigation must use this helper instead of calling `zoom_to_fit`
    /// immediately after mutating `editor_state`, otherwise the lazy scene
    /// gate can still expose the previously painted page's bounds.
    pub(in crate::widget_host) fn fit_active_page_after_switch(
        &mut self,
        viewport_w: f32,
        viewport_h: f32,
    ) {
        // A page switch changes the editor scene; a preview still owns its
        // old scene and must stop before the new page is painted.
        #[cfg(feature = "canvaskit")]
        self.finish_exit_teardown();
        self.layout_transition = None;
        self.mark_dirty();
        self.zoom_to_fit(viewport_w, viewport_h);
    }
}
