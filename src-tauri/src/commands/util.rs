use std::process::Stdio;
use tokio::process::Command;

/// Build a `tokio::process::Command` pre-configured with:
/// - CREATE_NO_WINDOW flag on Windows (prevents terminal flicker)
/// - stdout / stderr set to `Stdio::null()` by default
///
/// Caller may override stdio (e.g. `.stdout(Stdio::piped())`) before spawning.
pub fn silent_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd
}

/// Same as `silent_command` but wraps the program in `cmd /c <program>` on Windows.
/// Use this when the target is a `.cmd` / `.bat` script that `Command::new` may not
/// resolve directly (e.g. phpvm on Windows).
pub fn silent_shell_command(program: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = Command::new("cmd");
        cmd.args(["/c", program])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = Command::new(program);
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        cmd
    }
}
