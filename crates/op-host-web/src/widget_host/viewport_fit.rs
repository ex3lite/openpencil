//! Active-page viewport fitting shared by page-navigation entry points.

use super::WidgetHost;

impl WidgetHost {
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
