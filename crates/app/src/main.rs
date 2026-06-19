//! Entry point của myTerm2.
//!
//! Khởi tạo application, đăng ký UI, mở window chính.

use gpui_component_assets::Assets;
use myterm2_ui::layout::MyTermWorkspace;

mod window;

fn main() {
    let app = gpui_platform::application().with_assets(Assets);

    app.run(move |cx| {
        // Khởi tạo gpui-component (theme, dock, root, ...).
        gpui_component::init(cx);
        // Khởi tạo UI myTerm2 (register_panel x3, theme action handlers).
        myterm2_ui::init(cx);
        // Bind key bindings cho workspace.
        MyTermWorkspace::bind_keys(cx);

        cx.activate(true);

        // Mở window chính.
        crate::window::open_window(cx).detach();
    });
}
