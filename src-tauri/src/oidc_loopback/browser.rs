#[cfg(windows)]
pub(super) fn open(url: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    let target: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as isize;
    if code <= 32 {
        eprintln!("[Recorder][AuthHealth] stage=oidc_browser_launch ok=false code={code}");
        return Err("Windows could not open the configured identity provider".to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn open(_url: &str) -> Result<(), String> {
    Err("Native OIDC browser launch is currently implemented only on Windows".to_string())
}
