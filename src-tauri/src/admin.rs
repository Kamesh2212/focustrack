//! Administrator mode (#5, Genshin Impact report).
//!
//! Some games run as administrator (Genshin Impact, many anti-cheat titles).
//! Windows doesn't let a normal app close them, so blocking needs FocusTrack to
//! run as administrator too. Turning this on creates a scheduled task (one UAC
//! prompt) that starts FocusTrack with highest privileges at sign-in, and
//! restarts FocusTrack that way. Turning it off removes the task.

use crate::platform;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const TASK: &str = "FocusTrack";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

pub fn task_installed() -> bool {
    hidden(Command::new("schtasks").args(["/Query", "/TN", TASK]))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The `schtasks /Create` arguments for this exe.
pub fn create_args(exe: &str) -> String {
    format!(r#"/Create /TN {TASK} /TR "\"{exe}\" --minimized" /SC ONLOGON /RL HIGHEST /F"#)
}

/// Asks for administrator permission and creates the start-up task.
pub fn enable() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    match platform::run_elevated("schtasks.exe", &create_args(&exe.to_string_lossy())) {
        Some(0) => Ok(()),
        Some(code) => Err(format!(
            "Windows couldn't create the start-up task (code {code})"
        )),
        None => Err("Administrator permission wasn't given".into()),
    }
}

pub fn disable() -> Result<(), String> {
    match platform::run_elevated("schtasks.exe", &format!("/Delete /TN {TASK} /F")) {
        Some(0) => Ok(()),
        Some(code) => Err(format!(
            "Windows couldn't remove the start-up task (code {code})"
        )),
        None => Err("Administrator permission wasn't given".into()),
    }
}

/// Starts the administrator copy (via the task) after this one has quit.
pub fn restart_elevated() {
    let _ = hidden(Command::new("powershell.exe").args([
        "-NoProfile",
        "-Command",
        &format!("Start-Sleep -Seconds 2; schtasks /Run /TN {TASK}"),
    ]))
    .spawn();
}

/// Starts a normal (non-administrator) copy after this one has quit.
/// Going through Explorer drops administrator rights.
pub fn restart_normal() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = hidden(Command::new("powershell.exe").args([
            "-NoProfile",
            "-Command",
            &format!(
                "Start-Sleep -Seconds 2; explorer.exe '{}'",
                exe.to_string_lossy()
            ),
        ]))
        .spawn();
    }
}

/// Asks a running FocusTrack (on the local API) to show its window.
pub fn ask_running_copy_to_show() -> bool {
    let Ok(mut s) = TcpStream::connect_timeout(
        &([127, 0, 0, 1], crate::profile::api_port()).into(),
        Duration::from_millis(500),
    ) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let req = format!(
        "POST /v1/show HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-FocusTrack: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        crate::profile::api_port()
    );
    if s.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out.starts_with("HTTP/1.1 200")
}

/// On start-up without administrator rights while administrator mode is on:
/// hand over to the administrator copy instead of running a weaker second one.
/// Returns true if this process should exit.
pub fn hand_over_if_needed() -> bool {
    if platform::is_elevated() || !task_installed() {
        return false;
    }
    if ask_running_copy_to_show() {
        return true;
    }
    let _ = hidden(Command::new("schtasks").args(["/Run", "/TN", TASK])).status();
    // Show its window once it's up (it starts minimised).
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(500));
        if ask_running_copy_to_show() {
            break;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_command_quotes_paths_with_spaces() {
        assert_eq!(
            create_args(r"C:\Users\A B\AppData\Local\FocusTrack\focustrack.exe"),
            r#"/Create /TN FocusTrack /TR "\"C:\Users\A B\AppData\Local\FocusTrack\focustrack.exe\" --minimized" /SC ONLOGON /RL HIGHEST /F"#
        );
    }
}
