//! Bind every child process to the life of the desktop process on Windows.
//!
//! `kill_on_drop(true)` on each child and the `ExitRequested` hook in `main`
//! cover the graceful paths: window close, Cmd+Q, the tray "Quitter". They
//! cover nothing when the host dies without running them: a crash, a
//! `taskkill /F`, the terminal closing under a `cargo tauri dev` session, an
//! installer's restart manager. Windows has no parent-death signal, so the
//! children simply stay. Measured on 2026-09-09 on a Windows 11 machine: six
//! `apollia-runner.exe` from development sessions of the day before were still
//! alive, and a `llama-server` outlived a hard kill of the desktop.
//!
//! A surviving `llama-server` is not idle weight: it keeps the model's weights
//! resident on the card, and the next instance then shares the card with it.
//! Measured the same day, same 20 GB model on a 16 GB card: 36 tokens per
//! second alone, 14.7 with a leftover instance loaded, 7.6 with both
//! generating.
//!
//! A job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` is what Windows
//! offers for this. The host puts itself in the job at startup; every process
//! it spawns afterwards inherits the membership; when the last handle to the
//! job closes, which happens when the host dies by any means, the kernel
//! terminates every member. Nothing else has to remember to kill anything.
//!
//! Raw `kernel32` declarations rather than a bindings crate: four functions
//! and one struct, no new dependency and no new sovereignty surface, the same
//! choice `apollia_core::subprocess_window` made for its single constant.
//! A no-op everywhere but Windows: on Unix the supervisors' `kill_on_drop`
//! and the exit hook are what the platform offers and what the tree relies on.

/// Why the host could not be placed in a kill-on-close job.
///
/// Carries the Win32 error code so the log line names the cause. None of these
/// stops the application: without the job the tree behaves as it did before,
/// children are killed on graceful exits only.
// Only Windows constructs these; elsewhere the enum documents the contract
// and stays uninhabited by any code path, which the lint reads as dead.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProcessJobError {
    /// `CreateJobObjectW` returned no handle.
    #[error("CreateJobObjectW failed (win32 error {0})")]
    Create(u32),
    /// `SetInformationJobObject` refused the kill-on-close limit.
    #[error("SetInformationJobObject failed (win32 error {0})")]
    Configure(u32),
    /// `AssignProcessToJobObject` refused the process, typically because it
    /// already sits in a job that allows neither nesting nor breakaway.
    #[error("AssignProcessToJobObject failed (win32 error {0})")]
    Assign(u32),
}

/// Put this process in a job that kills every member when its last handle
/// closes, so children spawned from now on die with the host, however it dies.
///
/// Call once, early in `main`, before any child is spawned: membership is
/// inherited at process creation and never applied retroactively. The job
/// handle is deliberately never closed: closing it is what terminates the
/// members, and the only closing that should do so is the kernel's, when this
/// process ends.
pub fn bind_children_to_this_process() -> Result<(), ProcessJobError> {
    #[cfg(windows)]
    {
        let job = windows::Job::create_kill_on_close()?;
        // SAFETY: GetCurrentProcess returns a pseudo-handle that is always
        // valid for the calling process and never needs closing.
        let this_process = unsafe { windows::GetCurrentProcess() };
        job.assign(this_process)?;
        // The handle must outlive every child; leaking it is the intent.
        std::mem::forget(job);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) mod windows {
    use super::ProcessJobError;
    use std::ffi::c_void;

    /// A Win32 `HANDLE`.
    pub type Handle = *mut c_void;

    /// `JobObjectExtendedLimitInformation` in the `JOBOBJECTINFOCLASS` enum.
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;
    /// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` in `LimitFlags`.
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;

    /// `JOBOBJECT_BASIC_LIMIT_INFORMATION`, laid out as the Win32 headers do.
    #[repr(C)]
    #[derive(Default)]
    struct BasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    /// `IO_COUNTERS`, laid out as the Win32 headers do.
    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    /// `JOBOBJECT_EXTENDED_LIMIT_INFORMATION`, laid out as the Win32 headers do.
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimitInformation {
        basic_limit_information: BasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: u32,
            info: *const c_void,
            info_len: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        pub fn GetCurrentProcess() -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
        fn GetLastError() -> u32;
    }

    /// An owned job object handle. Dropping it closes the handle, which, with
    /// the kill-on-close limit set, terminates every member.
    pub struct Job(Handle);

    impl Job {
        /// Create an anonymous job whose members die when its last handle
        /// closes.
        pub fn create_kill_on_close() -> Result<Self, ProcessJobError> {
            // SAFETY: both pointers may be null per the Win32 contract: no
            // security attributes, no name. The returned handle is checked
            // before use.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                // SAFETY: GetLastError takes no arguments and only reads
                // thread-local state.
                return Err(ProcessJobError::Create(unsafe { GetLastError() }));
            }
            let job = Job(handle);

            let mut info = ExtendedLimitInformation::default();
            info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let info_len = std::mem::size_of::<ExtendedLimitInformation>();
            // SAFETY: `info` is a live, correctly laid out
            // JOBOBJECT_EXTENDED_LIMIT_INFORMATION and `info_len` is its exact
            // size, which is what the class asks for. The handle is the valid
            // one created above. `u32::try_from` cannot fail on a struct of a
            // few dozen bytes; the fallback keeps the call well-formed anyway.
            let ok = unsafe {
                SetInformationJobObject(
                    job.0,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    (&info as *const ExtendedLimitInformation).cast::<c_void>(),
                    u32::try_from(info_len).unwrap_or(u32::MAX),
                )
            };
            if ok == 0 {
                // SAFETY: as above, GetLastError only reads thread-local state.
                return Err(ProcessJobError::Configure(unsafe { GetLastError() }));
            }
            Ok(job)
        }

        /// Make `process` a member of this job. Children it spawns afterwards
        /// are members too.
        pub fn assign(&self, process: Handle) -> Result<(), ProcessJobError> {
            // SAFETY: both handles are valid: the job was created by this
            // type and is not yet closed, and `process` is a handle the caller
            // obtained from the system with at least PROCESS_SET_QUOTA and
            // PROCESS_TERMINATE rights, which a pseudo-handle and a `Child`
            // handle both carry.
            let ok = unsafe { AssignProcessToJobObject(self.0, process) };
            if ok == 0 {
                // SAFETY: GetLastError only reads thread-local state.
                return Err(ProcessJobError::Assign(unsafe { GetLastError() }));
            }
            Ok(())
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle was returned by CreateJobObjectW and is closed
            // exactly once, here.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::windows::io::AsRawHandle;
        use std::time::{Duration, Instant};

        /// A child that would live for thirty seconds if nothing stops it.
        fn spawn_long_child() -> std::process::Child {
            let mut cmd = std::process::Command::new("cmd.exe");
            cmd.args(["/c", "ping -n 30 127.0.0.1 > nul"]);
            apollia_core::subprocess_window::hide_console(&mut cmd);
            cmd.spawn().expect("spawn cmd.exe")
        }

        /// Whether the child has exited within `within`.
        fn exited(child: &mut std::process::Child, within: Duration) -> bool {
            let deadline = Instant::now() + within;
            while Instant::now() < deadline {
                if child.try_wait().expect("try_wait").is_some() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            false
        }

        #[test]
        fn a_member_dies_when_the_job_closes() {
            // GIVEN a kill-on-close job with a long-lived child assigned to it
            let job = Job::create_kill_on_close().expect("create job");
            let mut child = spawn_long_child();
            job.assign(child.as_raw_handle().cast::<c_void>())
                .expect("assign child");

            // WHEN the last handle to the job closes
            drop(job);

            // THEN the child is terminated by the kernel, without anyone
            // calling kill on it
            assert!(
                exited(&mut child, Duration::from_secs(5)),
                "a member must not outlive the job"
            );
        }

        #[test]
        fn a_process_outside_the_job_survives_the_job_closing() {
            // GIVEN a kill-on-close job and a long-lived child NOT assigned to
            // it. This is the control for the test above: it shows the job is
            // what kills, not the spawn or the drop.
            let job = Job::create_kill_on_close().expect("create job");
            let mut child = spawn_long_child();

            // WHEN the job closes
            drop(job);

            // THEN the child is still running a second later
            assert!(
                !exited(&mut child, Duration::from_secs(1)),
                "a non-member must be untouched by the job closing"
            );
            child.kill().expect("kill control child");
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_answers_on_every_platform() {
        // GIVEN this process, on the platform the test was compiled for
        // WHEN it asks to bind its future children to its own life
        let outcome = bind_children_to_this_process();

        // THEN the call is a no-op success outside Windows, and on Windows it
        // either succeeds or names the Win32 refusal, which happens when the
        // test runner itself sits in a job that forbids nesting. Both are
        // valid answers; a panic or a hang is not.
        #[cfg(not(windows))]
        assert!(outcome.is_ok(), "nothing to do outside Windows");
        #[cfg(windows)]
        if let Err(err) = outcome {
            assert!(
                matches!(err, ProcessJobError::Assign(_)),
                "only the assignment may be refused by the environment: {err}"
            );
        }
    }
}
