//! A Windows job object that caps a worker's memory and ends the worker when
//! the main process exits (design §5.3).
//!
//! This is the only module in the workspace allowed to use `unsafe`: no
//! maintained safe wrapper sets `JOB_OBJECT_LIMIT_PROCESS_MEMORY` (win32job
//! 2.0 only offers the working-set limit, which trims memory instead of
//! failing allocations). Each call below passes either a handle this module
//! owns or one borrowed from a live `Child`.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::process::Child;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
};

/// An owned job object handle, closed on drop. Closing the last handle ends
/// every process in the job because of `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
pub struct Job(HANDLE);

// SAFETY: a job object handle is a process-wide kernel handle; Windows allows
// it to be used and closed from any thread.
unsafe impl Send for Job {}

impl Job {
    /// Creates an unnamed job whose processes may each commit at most
    /// `limit` bytes and are killed when the job is closed.
    pub fn with_memory_limit(limit: u64) -> io::Result<Self> {
        // SAFETY: null attributes and a null name create a private job with
        // the default security descriptor.
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);

        // SAFETY: an all-zero JOBOBJECT_EXTENDED_LIMIT_INFORMATION is the
        // documented "no limits" value; the fields we need are set below.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        info.ProcessMemoryLimit = usize::try_from(limit).unwrap_or(usize::MAX);
        let size = u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(|_| io::Error::other("limit information is too large"))?;

        // SAFETY: `job.0` is a valid job handle owned by `job`, and `info`
        // is a properly initialised struct of exactly `size` bytes that
        // outlives the call.
        let ok = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                ptr::from_ref(&info).cast::<c_void>(),
                size,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    /// Puts `child` into this job.
    pub fn assign(&self, child: &Child) -> io::Result<()> {
        // SAFETY: `self.0` is a valid job handle and the process handle is
        // borrowed from `child`, which is alive for the whole call.
        let ok = unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a handle this struct owns and closes only here.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use super::Job;

    #[test]
    fn closing_the_job_ends_its_processes() {
        // Any long-running process shows the effect; ping waits about 30 s.
        let mut child = Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let job = Job::with_memory_limit(crate::server::WORKER_MEMORY_LIMIT).unwrap();
        job.assign(&child).unwrap();
        drop(job);
        let deadline = Instant::now() + Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "process outlived its job");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
