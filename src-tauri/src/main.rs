// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    unsafe {
        std::env::set_var(
            "GST_PLUGIN_PATH",
            "/home/user/.local/share/opal-matrix/libgsttauriasset.so",
        );
        std::env::set_var("WEBKIT_GST_ALLOWED_URI_PROTOCOLS", "mxc");
    }
    some_matrix_frontend_lib::run()
}
