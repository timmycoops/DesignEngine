//! Zero-copy hand-over of render buffers through WebView2 shared memory (Windows).
//!
//! The bytes are copied once into a shared-memory section and posted to the page, which receives
//! an `ArrayBuffer` over that same memory: no IPC serialisation and no second copy. The page
//! releases the memory with `chrome.webview.releaseBuffer` when the mesh is removed.

/// Posts `bytes` to the page as a `sharedbufferreceived` event carrying `additional_data` (JSON).
/// Blocks until the UI thread has posted it; call it off the async runtime.
#[cfg(windows)]
pub fn post_shared_buffer(webview: &tauri::Webview, bytes: Vec<u8>, additional_data: String) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    webview
        .with_webview(move |platform| {
            // WebView2 objects belong to the UI thread, where this closure runs.
            let _ = tx.send(post_on_ui_thread(&platform, &bytes, &additional_data));
        })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

#[cfg(windows)]
fn post_on_ui_thread(platform: &tauri::webview::PlatformWebview, bytes: &[u8], additional_data: &str) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_SHARED_BUFFER_ACCESS_READ_ONLY, ICoreWebView2_17, ICoreWebView2Environment12,
    };
    use windows_core::{HSTRING, Interface, PCWSTR};

    let fail = |what: &'static str| move |e: windows_core::Error| format!("{what}: {e}");
    // SAFETY: COM calls on the UI thread that owns the WebView2 objects. `memory` is valid for
    // `bytes.len()` bytes once `Buffer` succeeds, and nothing else writes it before the post.
    unsafe {
        let environment: ICoreWebView2Environment12 =
            platform.environment().cast().map_err(fail("this WebView2 runtime has no shared buffers"))?;
        let core: ICoreWebView2_17 = platform
            .controller()
            .CoreWebView2()
            .map_err(fail("CoreWebView2"))?
            .cast()
            .map_err(fail("this WebView2 runtime has no shared buffers"))?;
        let shared = environment.CreateSharedBuffer(bytes.len() as u64).map_err(fail("CreateSharedBuffer"))?;
        let mut memory: *mut u8 = std::ptr::null_mut();
        shared.Buffer(&mut memory).map_err(fail("shared buffer address"))?;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), memory, bytes.len());
        let data = HSTRING::from(additional_data);
        core.PostSharedBufferToScript(&shared, COREWEBVIEW2_SHARED_BUFFER_ACCESS_READ_ONLY, PCWSTR(data.as_ptr()))
            .map_err(fail("PostSharedBufferToScript"))
    }
}

#[cfg(not(windows))]
pub fn post_shared_buffer(_webview: &tauri::Webview, _bytes: Vec<u8>, _additional_data: String) -> Result<(), String> {
    Err("shared buffers need WebView2 (Windows)".into())
}
