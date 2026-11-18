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
    close_windows, file_description, foreground_app, foreground_pid, idle_seconds,
    is_session_locked, process_image, processes, terminate,
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
    stem.split(['_', '-', ' '])
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
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
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
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetForegroundWindow, GetWindowThreadProcessId,
        IsWindowVisible, PostMessageW, WM_CLOSE,
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

    /// Reads the `FileDescription` from the exe's version resource
    /// (e.g. `chrome.exe` -> "Google Chrome").
    pub fn file_description(path: &str) -> Option<String> {
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
                    "\\StringFileInfo\\{:04x}{:04x}\\FileDescription",
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
    pub fn foreground_app() -> Option<ForegroundApp> {
        None
    }
    pub fn idle_seconds() -> u64 {
        0
    }
    pub fn is_session_locked() -> bool {
        false
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

    #[test]
    fn pretty_names() {
        assert_eq!(pretty_exe_name("my_cool-app.exe"), "My Cool App");
        assert_eq!(pretty_exe_name("discord.exe"), "Discord");
    }
}
