use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr;
use tracing::info;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicUIRestrictions,
    JobObjectExtendedLimitInformation, SetInformationJobObject, JOBOBJECT_BASIC_UI_RESTRICTIONS,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    JOB_OBJECT_UILIMIT_DESKTOP, JOB_OBJECT_UILIMIT_EXITWINDOWS, JOB_OBJECT_UILIMIT_HANDLES,
};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};

use crate::{SandboxError, SandboxResult};

/// A process sandbox enforced by Windows OS Job Objects.
pub struct JobObjectSandbox {
    job_handle: HANDLE,
    pub name: String,
}

impl std::fmt::Debug for JobObjectSandbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobObjectSandbox")
            .field("job_handle", &"HANDLE")
            .field("name", &self.name)
            .finish()
    }
}

impl JobObjectSandbox {
    /// Create a new Job Object
    pub fn new(name: &str) -> SandboxResult<Self> {
        let name_wide: Vec<u16> = OsStr::new(name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // SAFETY: Calling Windows API CreateJobObjectW. We pass a null security attribute
        // so it gets a default descriptor, and the wide string name.
        let job_handle = unsafe { CreateJobObjectW(ptr::null(), name_wide.as_ptr()) };

        if job_handle == 0 {
            return Err(SandboxError::CreationFailed(
                std::io::Error::last_os_error().to_string(),
            ));
        }

        info!("Created Windows Job Object: {}", name);
        Ok(Self {
            job_handle,
            name: name.to_string(),
        })
    }

    /// Configure the Job Object with strict limits
    pub fn configure(&self) -> SandboxResult<()> {
        let mut extended_info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };

        // Kill child processes when job handle is closed
        extended_info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY;

        // Limit to 512 MB
        extended_info.ProcessMemoryLimit = 512 * 1024 * 1024;

        // SAFETY: Calling Windows API SetInformationJobObject with valid handle and buffer
        let result = unsafe {
            SetInformationJobObject(
                self.job_handle,
                JobObjectExtendedLimitInformation,
                &extended_info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };

        if result == 0 {
            return Err(SandboxError::CreationFailed(format!(
                "Failed to set extended limit info: {}",
                std::io::Error::last_os_error()
            )));
        }

        let mut ui_restrictions: JOBOBJECT_BASIC_UI_RESTRICTIONS = unsafe { std::mem::zeroed() };
        ui_restrictions.UIRestrictionsClass = JOB_OBJECT_UILIMIT_DESKTOP
            | JOB_OBJECT_UILIMIT_EXITWINDOWS
            | JOB_OBJECT_UILIMIT_HANDLES;

        // SAFETY: Calling Windows API SetInformationJobObject with valid handle and buffer
        let result = unsafe {
            SetInformationJobObject(
                self.job_handle,
                JobObjectBasicUIRestrictions,
                &ui_restrictions as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32,
            )
        };

        if result == 0 {
            return Err(SandboxError::CreationFailed(format!(
                "Failed to set UI restrictions: {}",
                std::io::Error::last_os_error()
            )));
        }

        info!("Configured strict limits for Job Object {}", self.name);
        Ok(())
    }

    /// Assign a given process PID to this Job Object
    pub fn assign_process(&self, pid: u32) -> SandboxResult<()> {
        // SAFETY: Calling OpenProcess to get handle for specific PID
        let process_handle = unsafe { OpenProcess(PROCESS_ALL_ACCESS, 0, pid) };

        if process_handle == 0 || process_handle == INVALID_HANDLE_VALUE {
            return Err(SandboxError::CreationFailed(format!(
                "Failed to open process {}: {}",
                pid,
                std::io::Error::last_os_error()
            )));
        }

        // SAFETY: Calling AssignProcessToJobObject with valid job and process handles
        let result = unsafe { AssignProcessToJobObject(self.job_handle, process_handle) };

        // SAFETY: Closing the process handle after assignment
        unsafe { CloseHandle(process_handle) };

        if result == 0 {
            return Err(SandboxError::CreationFailed(format!(
                "Failed to assign process {} to job: {}",
                pid,
                std::io::Error::last_os_error()
            )));
        }

        info!("Assigned process {} to Job Object {}", pid, self.name);
        Ok(())
    }

    /// Close the job handle. Usually called automatically on drop.
    pub fn close(&self) {
        if self.job_handle != 0 {
            // SAFETY: Calling CloseHandle with the valid job handle
            unsafe { CloseHandle(self.job_handle) };
            info!("Job object {} closed", self.name);
        }
    }
}

impl Drop for JobObjectSandbox {
    fn drop(&mut self) {
        self.close();
    }
}
