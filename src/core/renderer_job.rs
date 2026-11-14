//! Windows owns renderer descendants through a job, including when the backend is terminated.

use std::io;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

pub struct RendererJob(HANDLE);

impl RendererJob {
    pub fn attach(child: &tokio::process::Child) -> io::Result<Self> {
        // SAFETY: default security, unnamed job. The handle is owned here and closed on every exit.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the initialized information structure has the exact size required by this class.
        if unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let process = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("renderer already exited"))?;
        // SAFETY: Tokio owns the live child process handle; assigning it does not transfer ownership.
        if unsafe { AssignProcessToJobObject(handle, process.cast()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
}

impl Drop for RendererJob {
    fn drop(&mut self) {
        // SAFETY: this job owns this handle, closed once. Its renderer cannot outlive the backend.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
