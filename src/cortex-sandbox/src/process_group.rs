//! Safe ownership guard for a child launched in its own Unix process group.
use std::io;

/// Owns the lifetime of a process group created with `Command::process_group(0)`.
/// Keep this guard alive until output and wait processing have finished.
#[derive(Debug)]
pub struct ProcessGroup {
    id: libc::pid_t,
}

impl ProcessGroup {
    pub fn new(child_pid: u32) -> io::Result<Self> {
        let id = libc::pid_t::try_from(child_pid)
            .map_err(|_| io::Error::other("Invalid child process group"))?;
        if id <= 1 {
            return Err(io::Error::other("Invalid child process group"));
        }
        Ok(Self { id })
    }

    /// Kill all members, including grandchildren, not just the shell leader.
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: a strictly positive stored group ID prevents signaling the
        // caller's process group or the special all-processes target.
        let result = unsafe { libc::kill(-self.id, libc::SIGKILL) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error)
        }
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_boundary_rejects_special_process_groups() {
        assert!(ProcessGroup::new(0).is_err());
        assert!(ProcessGroup::new(1).is_err());
        assert!(ProcessGroup::new(u32::MAX).is_err());
    }
}
