//! Thin wrappers around the Windows APIs FocusTrack needs.
//!
//! Privacy: nothing here reads keystrokes, window contents or screenshots.
//! Idle detection uses `GetLastInputInfo`, which only reports *when* the last
//! input happened, never *what* it was.

#[derive(Debug, Clone)]
pub struct ForegroundApp {
    /// e.g. `chrome.exe`
    pub exe_name: String,
    /// Full path to the executable.
    pub exe_path: String,
}

pub use imp::{
    audible_exes, close_windows, controller_idle_seconds, file_description, foreground_app,
    foreground_pid, has_modal_dialog, idle_seconds, is_elevated, is_session_locked,
    original_file_name, process_image, processes, run_elevated, terminate, uptime_ms,
};

/// Returns the last path component (`C:\a\b.exe` -> `b.exe`).
pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// Fallback display name built from the exe name: `my_app.exe` -> `My App`.
pub fn pretty_exe_name(exe_name: &str) -> String {
    let stem = exe_name
        .strip_suffix(".exe")
        .or_else(|| exe_name.strip_suffix(".EXE"))
        .unwrap_or(exe_name);
    // Split CamelCase ("GenshinImpact" -> "Genshin Impact") before separators.
    let mut spaced = String::new();
    let chars: Vec<char> = stem.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let prev_lower = i > 0 && chars[i - 1].is_lowercase();
        if c.is_uppercase() && prev_lower {
            spaced.push(' ');
        }
        spaced.push(*c);
    }
    spaced
        .split(['_', '-', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
mod imp {
    use super::{file_name, ForegroundApp};
    use std::ffi::c_void;
    use std::ptr::null_mut;
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, OpenInputDesktop, SwitchDesktop,
    };
    use windows_sys::Win32::System::SystemInformation::{GetTickCount, GetTickCount64};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetLastInputInfo, IsWindowEnabled, LASTINPUTINFO,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetForegroundWindow, GetWindow, GetWindowThreadProcessId,
        IsWindowVisible, PostMessageW, GW_OWNER, WM_CLOSE,
    };

    const PROCESS_TERMINATE: u32 = 0x0001;

    /// All running processes as (pid, exe name).
    pub fn processes() -> Vec<(u32, String)> {
        let mut out = Vec::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap.is_null() || snap == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut entry) != 0;
            while ok {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                out.push((
                    entry.th32ProcessID,
                    String::from_utf16_lossy(&entry.szExeFile[..len]),
                ));
                ok = Process32NextW(snap, &mut entry) != 0;
            }
            CloseHandle(snap);
        }
        out
    }

    pub fn process_image(pid: u32) -> Option<String> {
        unsafe { process_path(pid) }
    }

    pub fn foreground_pid() -> Option<u32> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            (pid != 0).then_some(pid)
        }
    }

    struct CloseSearch {
        pid: u32,
        sent: u32,
    }

    unsafe extern "system" fn close_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let s = &mut *(lparam as *mut CloseSearch);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == s.pid && IsWindowVisible(hwnd) != 0 {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
            s.sent += 1;
        }
        1
    }

    /// Politely asks a process's visible windows to close (lets apps offer to
    /// save). Returns how many windows were asked.
    pub fn close_windows(pid: u32) -> u32 {
        let mut s = CloseSearch { pid, sent: 0 };
        unsafe {
            EnumWindows(Some(close_cb), &mut s as *mut CloseSearch as LPARAM);
        }
        s.sent
    }

    struct ModalSearch {
        pid: u32,
        found: bool,
    }

    unsafe extern "system" fn modal_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let s = &mut *(lparam as *mut ModalSearch);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == s.pid && IsWindowVisible(hwnd) != 0 {
            let owner = GetWindow(hwnd, GW_OWNER);
            // A modal dialog disables the window it belongs to.
            if !owner.is_null() && IsWindowEnabled(owner) == 0 {
                s.found = true;
                return 0;
            }
        }
        1
    }

    /// Whether the process shows a modal dialog, such as "Save changes?".
    pub fn has_modal_dialog(pid: u32) -> bool {
        let mut s = ModalSearch { pid, found: false };
        unsafe {
            EnumWindows(Some(modal_cb), &mut s as *mut ModalSearch as LPARAM);
        }
        s.found
    }

    /// Force-closes a process. Used only after `close_windows` was ignored.
    pub fn terminate(pid: u32) -> bool {
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if h.is_null() {
                return false;
            }
            let ok = TerminateProcess(h, 1) != 0;
            CloseHandle(h);
            ok
        }
    }

    const DESKTOP_SWITCHDESKTOP: u32 = 0x0100;

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn process_path(pid: u32) -> Option<String> {
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        if ok == 0 || len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }

    struct ChildSearch {
        host_pid: u32,
        found_pid: u32,
    }

    unsafe extern "system" fn enum_child(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam as *mut ChildSearch);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != 0 && pid != search.host_pid {
            search.found_pid = pid;
            return 0; // stop enumerating
        }
        1
    }

    /// Store/UWP apps are hosted by ApplicationFrameHost.exe; the real app
    /// owns a child window of the frame.
    unsafe fn uwp_child_path(hwnd: HWND, host_pid: u32) -> Option<String> {
        let mut search = ChildSearch {
            host_pid,
            found_pid: 0,
        };
        EnumChildWindows(
            hwnd,
            Some(enum_child),
            &mut search as *mut ChildSearch as LPARAM,
        );
        if search.found_pid == 0 {
            None
        } else {
            process_path(search.found_pid)
        }
    }

    pub fn foreground_app() -> Option<ForegroundApp> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let mut path = process_path(pid)?;
            if file_name(&path).eq_ignore_ascii_case("ApplicationFrameHost.exe") {
                if let Some(child) = uwp_child_path(hwnd, pid) {
                    path = child;
                }
            }
            Some(ForegroundApp {
                exe_name: file_name(&path).to_string(),
                exe_path: path,
            })
        }
    }

    /// Lower-case exe names of processes currently playing sound (Windows audio
    /// sessions on the default output device with a non-zero peak level).
    /// Used so watching videos or playing games doesn't count as idle.
    pub fn audible_exes() -> std::collections::HashSet<String> {
        audio::audible_pids()
            .into_iter()
            .filter_map(|pid| unsafe { process_path(pid) })
            .map(|p| file_name(&p).to_ascii_lowercase())
            .collect()
    }

    mod audio {
        use std::cell::Cell;
        use windows::core::Interface;
        use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
        use windows::Win32::Media::Audio::{
            eConsole, eRender, AudioSessionStateActive, IAudioSessionControl2,
            IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator,
        };
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
        };

        thread_local!(static COM_READY: Cell<bool> = const { Cell::new(false) });

        /// Peak level above which a session counts as audible (silence is ~0).
        const AUDIBLE: f32 = 0.001;

        pub fn audible_pids() -> Vec<u32> {
            COM_READY.with(|ready| {
                if !ready.get() {
                    // Fine if COM was already initialised on this thread.
                    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
                    ready.set(true);
                }
            });
            unsafe { list() }.unwrap_or_default()
        }

        unsafe fn list() -> windows::core::Result<Vec<u32>> {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
            let sessions = manager.GetSessionEnumerator()?;
            let mut out = Vec::new();
            for i in 0..sessions.GetCount()? {
                let Ok(session) = sessions.GetSession(i) else {
                    continue;
                };

                if session.GetState().ok() != Some(AudioSessionStateActive) {
                    continue;
                }
                let peak = session
                    .cast::<IAudioMeterInformation>()
                    .and_then(|m| m.GetPeakValue())
                    .unwrap_or(0.0);
                if peak < AUDIBLE {
                    continue;
                }
                if let Ok(pid) = session
                    .cast::<IAudioSessionControl2>()
                    .and_then(|c| c.GetProcessId())
                {
                    if pid != 0 {
                        out.push(pid);
                    }
                }
            }
            Ok(out)
        }
    }

    /// Seconds since the last Xbox-compatible (XInput) controller input, or
    /// `u64::MAX` if none has been seen. `GetLastInputInfo` ignores gamepads,
    /// so without this, controller gaming would count as idle.
    pub fn controller_idle_seconds() -> u64 {
        use std::sync::Mutex;
        use std::time::Instant;
        use windows_sys::Win32::UI::Input::XboxController::{XInputGetState, XINPUT_STATE};
        static LAST: Mutex<([u32; 4], Option<Instant>)> = Mutex::new(([0; 4], None));
        let Ok(mut last) = LAST.lock() else {
            return u64::MAX;
        };
        for (i, seen) in last.0.clone().iter().enumerate() {
            let mut state: XINPUT_STATE = unsafe { std::mem::zeroed() };
            // 0 = ERROR_SUCCESS (controller connected)
            if unsafe { XInputGetState(i as u32, &mut state) } == 0 && state.dwPacketNumber != *seen
            {
                if *seen != 0 {
                    last.1 = Some(Instant::now());
                }
                last.0[i] = state.dwPacketNumber;
            }
        }
        last.1.map_or(u64::MAX, |t| t.elapsed().as_secs())
    }

    /// Milliseconds since Windows started, including time asleep. Unaffected by
    /// changes to the system clock.
    pub fn uptime_ms() -> u64 {
        unsafe { GetTickCount64() }
    }

    /// Seconds since the last keyboard/mouse input anywhere in the session.
    pub fn idle_seconds() -> u64 {
        unsafe {
            let mut info = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            if GetLastInputInfo(&mut info) == 0 {
                return 0;
            }
            (GetTickCount().wrapping_sub(info.dwTime) / 1000) as u64
        }
    }

    /// True while the workstation is locked (or the secure desktop is shown).
    pub fn is_session_locked() -> bool {
        unsafe {
            let desk = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
            if desk.is_null() {
                return true;
            }
            let switched = SwitchDesktop(desk) != 0;
            CloseDesktop(desk);
            !switched
        }
    }

    /// Whether FocusTrack itself is running with administrator rights.
    pub fn is_elevated() -> bool {
        use windows_sys::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
        unsafe {
            let mut token = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let mut info = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut len = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut info as *mut _ as *mut c_void,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut len,
            );
            CloseHandle(token);
            ok != 0 && info.TokenIsElevated != 0
        }
    }

    /// Runs a program with the Windows administrator prompt (UAC) and waits
    /// for it. Returns its exit code, or None if the user said no.
    pub fn run_elevated(file: &str, args: &str) -> Option<u32> {
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, WaitForSingleObject, INFINITE,
        };
        use windows_sys::Win32::UI::Shell::{
            ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
        };
        let verb = to_wide("runas");
        let file = to_wide(file);
        let params = to_wide(args);
        unsafe {
            let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
            info.fMask = SEE_MASK_NOCLOSEPROCESS;
            info.lpVerb = verb.as_ptr();
            info.lpFile = file.as_ptr();
            info.lpParameters = params.as_ptr();
            info.nShow = 0; // SW_HIDE
            if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
                return None;
            }
            WaitForSingleObject(info.hProcess, INFINITE);
            let mut code = 1u32;
            GetExitCodeProcess(info.hProcess, &mut code);
            CloseHandle(info.hProcess);
            Some(code)
        }
    }

    /// Reads the `FileDescription` from the exe's version resource
    /// (e.g. `chrome.exe` -> "Google Chrome").
    pub fn file_description(path: &str) -> Option<String> {
        version_field(path, "FileDescription")
    }

    /// The exe's original file name from its version resource; survives the
    /// file being renamed (used to recognise renamed copies of limited apps).
    pub fn original_file_name(path: &str) -> Option<String> {
        version_field(path, "OriginalFilename")
            .map(|n| n.trim().to_ascii_lowercase())
            .filter(|n| n.ends_with(".exe"))
    }

    fn version_field(path: &str, field: &str) -> Option<String> {
        unsafe {
            let wide = to_wide(path);
            let mut handle = 0u32;
            let size = GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle);
            if size == 0 {
                return None;
            }
            let mut data = vec![0u8; size as usize];
            if GetFileVersionInfoW(wide.as_ptr(), 0, size, data.as_mut_ptr() as *mut c_void) == 0 {
                return None;
            }
            let block = data.as_ptr() as *const c_void;

            let mut langs: Vec<(u16, u16)> = Vec::new();
            let mut ptr: *mut c_void = null_mut();
            let mut len = 0u32;
            let key = to_wide("\\VarFileInfo\\Translation");
            if VerQueryValueW(block, key.as_ptr(), &mut ptr, &mut len) != 0 && !ptr.is_null() {
                let n = len as usize / 4;
                let pairs = std::slice::from_raw_parts(ptr as *const u16, n * 2);
                for i in 0..n {
                    langs.push((pairs[2 * i], pairs[2 * i + 1]));
                }
            }
            langs.push((0x0409, 0x04B0));
            langs.push((0x0409, 0x04E4));

            for (lang, cp) in langs {
                let key = to_wide(&format!(
                    "\\StringFileInfo\\{:04x}{:04x}\\{field}",
                    lang, cp
                ));
                let mut p: *mut c_void = null_mut();
                let mut l = 0u32;
                if VerQueryValueW(block, key.as_ptr(), &mut p, &mut l) != 0 && !p.is_null() && l > 0
                {
                    let chars = std::slice::from_raw_parts(p as *const u16, l as usize);
                    let text = String::from_utf16_lossy(chars)
                        .trim_end_matches('\0')
                        .trim()
                        .to_string();
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
            None
        }
    }
}

/// Non-Windows stubs so the crate still type-checks elsewhere (CI, etc.).
#[cfg(not(windows))]
mod imp {
    use super::ForegroundApp;
    pub fn uptime_ms() -> u64 {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_millis() as u64
    }
    pub fn foreground_app() -> Option<ForegroundApp> {
        None
    }
    pub fn idle_seconds() -> u64 {
        0
    }
    pub fn is_session_locked() -> bool {
        false
    }
    pub fn audible_exes() -> std::collections::HashSet<String> {
        Default::default()
    }
    pub fn original_file_name(_path: &str) -> Option<String> {
        None
    }
    pub fn is_elevated() -> bool {
        false
    }
    pub fn run_elevated(_file: &str, _args: &str) -> Option<u32> {
        None
    }
    pub fn controller_idle_seconds() -> u64 {
        u64::MAX
    }
    pub fn file_description(_path: &str) -> Option<String> {
        None
    }
    pub fn processes() -> Vec<(u32, String)> {
        Vec::new()
    }
    pub fn process_image(_pid: u32) -> Option<String> {
        None
    }
    pub fn foreground_pid() -> Option<u32> {
        None
    }
    pub fn has_modal_dialog(_pid: u32) -> bool {
        false
    }
    pub fn close_windows(_pid: u32) -> u32 {
        0
    }
    pub fn terminate(_pid: u32) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_handles_both_separators() {
        assert_eq!(file_name(r"C:\Program Files\App\app.exe"), "app.exe");
        assert_eq!(file_name("/usr/bin/app"), "app");
        assert_eq!(file_name("app.exe"), "app.exe");
    }

    /// Manual check on a real PC: `cargo test audible -- --ignored --nocapture`
    /// while something is playing sound.
    /// Manual check: `FT_PID=1234 cargo test inspect_pid -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn inspect_pid() {
        let pid: u32 = std::env::var("FT_PID").unwrap().parse().unwrap();
        println!("image: {:?}", process_image(pid));
        println!(
            "snapshot name: {:?}",
            processes().into_iter().find(|p| p.0 == pid)
        );
    }

    #[test]
    #[ignore]
    fn audible_apps_now() {
        println!("audible: {:?}", audible_exes());
        println!("controller idle: {}", controller_idle_seconds());
    }

    /// Real check on Windows: a window with a modal dialog open is detected,
    /// the same window without one isn't. `cargo test modal -- --ignored`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn modal_dialog_detected() {
        use std::process::Command;
        use std::time::Duration;
        let script = |modal: bool| {
            format!(
                "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.Form;                  $f.Text = 'FT main'; $f.Show(); {}",
                if modal {
                    "$d = New-Object System.Windows.Forms.Form; $d.Text = 'Save changes?'; [void]$d.ShowDialog($f)"
                } else {
                    "[System.Windows.Forms.Application]::Run($f)"
                }
            )
        };
        for modal in [true, false] {
            let mut child = Command::new("powershell")
                .args(["-NoProfile", "-Command", &script(modal)])
                .spawn()
                .unwrap();
            let pid = child.id();
            let mut seen = false;
            for _ in 0..40 {
                std::thread::sleep(Duration::from_millis(500));
                if has_modal_dialog(pid) {
                    seen = true;
                    break;
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            assert_eq!(seen, modal, "modal = {modal}");
        }
    }

    #[test]
    fn pretty_names() {
        assert_eq!(pretty_exe_name("my_cool-app.exe"), "My Cool App");
        assert_eq!(pretty_exe_name("discord.exe"), "Discord");
        assert_eq!(pretty_exe_name("GenshinImpact.exe"), "Genshin Impact");
        assert_eq!(pretty_exe_name("HYP.exe"), "HYP");
    }
}
