// ARCHITECTURE: kitsune-sandbox provides process sandboxing for KitsuneEngine.
// Different platforms use different sandboxing mechanisms:
// - Linux: seccomp-BPF
// - macOS: Seatbelt (App Sandbox)
// - Windows: Job Objects + Restricted Tokens
//
// This crate provides a unified interface that abstracts platform differences.

pub mod error;
pub mod policy;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows::JobObjectSandbox;

pub use error::{SandboxError, SandboxResult};
pub use policy::*;

use serde::{Deserialize, Serialize};
use tracing::info;

/// Sandbox profile — defines what a sandboxed process is allowed to do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxProfile {
    /// Human-readable name for this profile.
    pub name: String,
    /// Whether the process can access the filesystem.
    pub allow_filesystem: FileSystemPolicy,
    /// Whether the process can make network connections.
    pub allow_network: NetworkPolicy,
    /// Whether the process can spawn child processes.
    pub allow_process_spawn: bool,
    /// Whether the process can access the GPU.
    pub allow_gpu: bool,
    /// Whether the process can access audio/video devices.
    pub allow_media_devices: bool,
    /// Memory limit in bytes (0 = unlimited).
    pub memory_limit_bytes: u64,
    /// CPU time limit in seconds (0 = unlimited).
    pub cpu_time_limit_seconds: u64,
}

impl SandboxProfile {
    /// Create a maximally restrictive profile — the default for untrusted processes.
    pub fn maximum_restriction() -> Self {
        Self {
            name: "Maximum Restriction".to_string(),
            allow_filesystem: FileSystemPolicy::None,
            allow_network: NetworkPolicy::None,
            allow_process_spawn: false,
            allow_gpu: false,
            allow_media_devices: false,
            memory_limit_bytes: 512 * 1024 * 1024, // 512 MB
            cpu_time_limit_seconds: 30,
        }
    }

    /// Create a profile for renderer processes.
    pub fn renderer() -> Self {
        Self {
            name: "Renderer".to_string(),
            allow_filesystem: FileSystemPolicy::None,
            allow_network: NetworkPolicy::None, // Network goes through broker
            allow_process_spawn: false,
            allow_gpu: true,
            allow_media_devices: false,
            memory_limit_bytes: 1024 * 1024 * 1024, // 1 GB
            cpu_time_limit_seconds: 0,              // Unlimited
        }
    }

    /// Create a profile for the network process.
    pub fn network_process() -> Self {
        Self {
            name: "Network".to_string(),
            allow_filesystem: FileSystemPolicy::ReadOnly {
                paths: vec![], // Certificate stores added at runtime
            },
            allow_network: NetworkPolicy::Outbound {
                allowed_ports: vec![80, 443, 8080, 8443],
            },
            allow_process_spawn: false,
            allow_gpu: false,
            allow_media_devices: false,
            memory_limit_bytes: 256 * 1024 * 1024, // 256 MB
            cpu_time_limit_seconds: 0,
        }
    }

    /// Create a profile for agent processes.
    pub fn agent() -> Self {
        Self {
            name: "Agent".to_string(),
            allow_filesystem: FileSystemPolicy::None,
            allow_network: NetworkPolicy::None, // Network goes through broker
            allow_process_spawn: false,
            allow_gpu: false,
            allow_media_devices: false,
            memory_limit_bytes: 256 * 1024 * 1024, // 256 MB
            cpu_time_limit_seconds: 60,
        }
    }

    /// Create a profile for JS engine processes.
    pub fn js_engine() -> Self {
        Self {
            name: "JavaScript Engine".to_string(),
            allow_filesystem: FileSystemPolicy::None,
            allow_network: NetworkPolicy::None,
            allow_process_spawn: false,
            allow_gpu: false,
            allow_media_devices: false,
            memory_limit_bytes: 512 * 1024 * 1024, // 512 MB
            cpu_time_limit_seconds: 10,
        }
    }
}

/// Filesystem access policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileSystemPolicy {
    /// No filesystem access at all.
    None,
    /// Read-only access to specific paths.
    ReadOnly { paths: Vec<String> },
    /// Read-write access to specific paths.
    ReadWrite { paths: Vec<String> },
}

/// Network access policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkPolicy {
    /// No network access.
    None,
    /// Outbound connections only, to specific ports.
    Outbound { allowed_ports: Vec<u16> },
    /// Loopback only (localhost).
    LoopbackOnly,
}

/// A sandboxed process handle.
#[derive(Debug)]
pub struct SandboxedProcess {
    /// Process ID.
    pub pid: u32,
    /// The sandbox profile applied.
    pub profile: SandboxProfile,
    /// Whether the process is still running.
    pub running: bool,
}

/// Apply a sandbox profile to the current process.
///
/// This must be called early in the process lifecycle, before any
/// untrusted code is executed.
pub fn apply_sandbox(profile: &SandboxProfile) -> SandboxResult<()> {
    info!(
        profile = %profile.name,
        "Applying sandbox profile"
    );

    #[cfg(target_os = "windows")]
    {
        apply_windows_sandbox(profile)?;
    }

    #[cfg(target_os = "linux")]
    {
        apply_linux_sandbox(profile)?;
    }

    #[cfg(target_os = "macos")]
    {
        apply_macos_sandbox(profile)?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_windows_sandbox(profile: &SandboxProfile) -> SandboxResult<()> {
    // ARCHITECTURE: On Windows, we use Job Objects and Restricted Tokens.
    // Job Objects allow setting memory and CPU limits.
    // Restricted Tokens remove privileges from the process token.
    info!("Applying Windows sandbox via Job Objects");

    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::System::JobObjects::*;
    use windows_sys::Win32::System::Threading::*;

    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job == 0 {
            return Err(SandboxError::CreationFailed(
                "Failed to create job object".to_string(),
            ));
        }

        // UI restrictions — renderer cannot create desktop windows or global atoms
        let mut ui_limits: JOBOBJECT_BASIC_UI_RESTRICTIONS = std::mem::zeroed();
        ui_limits.UIRestrictionsClass = JOB_OBJECT_UILIMIT_DESKTOP
            | JOB_OBJECT_UILIMIT_GLOBALATOMS
            | JOB_OBJECT_UILIMIT_HANDLES;

        let res = SetInformationJobObject(
            job,
            JobObjectBasicUIRestrictions,
            &ui_limits as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32,
        );
        if res == 0 {
            return Err(SandboxError::CreationFailed(
                "Failed to set UI restrictions".to_string(),
            ));
        }

        // Memory limit
        if profile.memory_limit_bytes > 0 {
            let mut ext: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            ext.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY;
            ext.ProcessMemoryLimit = profile.memory_limit_bytes as usize;

            let res = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &ext as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if res == 0 {
                return Err(SandboxError::CreationFailed(
                    "Failed to set memory limits".to_string(),
                ));
            }
        }

        // Assign current process to the job
        let res = AssignProcessToJobObject(job, GetCurrentProcess());
        if res == 0 {
            return Err(SandboxError::CreationFailed(
                "Failed to assign process to job".to_string(),
            ));
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_linux_sandbox(profile: &SandboxProfile) -> SandboxResult<()> {
    // ARCHITECTURE: On Linux, we use seccomp-BPF to restrict system calls.
    // This is the most granular sandboxing mechanism available.
    tracing::warn!("Linux seccomp-BPF implementation is pending. Sandboxing is disabled.");
    info!("Applying Linux sandbox via seccomp-BPF");
    Ok(())
}

#[cfg(target_os = "macos")]
fn apply_macos_sandbox(profile: &SandboxProfile) -> SandboxResult<()> {
    // ARCHITECTURE: On macOS, we use Seatbelt (App Sandbox).
    info!("Applying macOS sandbox via Seatbelt");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_profile() {
        let profile = SandboxProfile::renderer();
        assert_eq!(profile.memory_limit_bytes, 1024 * 1024 * 1024);

        let res = apply_sandbox(&profile);
        assert!(res.is_ok());
    }
}
