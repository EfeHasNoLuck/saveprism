// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `saveprism-desktop.exe mcp` is the MCP server on stdio and never opens a window.
    if std::env::args_os()
        .nth(1)
        .is_some_and(|argument| argument.as_os_str() == std::ffi::OsStr::new("mcp"))
    {
        std::process::exit(saveprism_desktop_lib::mcp::run_stdio());
    }
    saveprism_desktop_lib::run()
}
