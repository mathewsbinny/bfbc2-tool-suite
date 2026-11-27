//! The machine's memory and a step's peak (Windows): free and used physical memory (`GlobalMemoryStatusEx`), and a
//! job object around each step's process tree, whose peak commit is the step's memory (every process the step
//! starts is in it). On other systems nothing is measured.

use std::process::Child;

/// Physical memory: (total, available), bytes.
#[cfg(windows)]
#[allow(unsafe_code)]
#[must_use]
pub fn physical() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut s = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(64),
        dwMemoryLoad: 0,
        ullTotalPhys: 0,
        ullAvailPhys: 0,
        ullTotalPageFile: 0,
        ullAvailPageFile: 0,
        ullTotalVirtual: 0,
        ullAvailVirtual: 0,
        ullAvailExtendedVirtual: 0,
    };
    // SAFETY: s is a MEMORYSTATUSEX with dwLength set, as the call requires
    let ok = unsafe { GlobalMemoryStatusEx(&raw mut s) };
    (ok != 0).then_some((s.ullTotalPhys, s.ullAvailPhys))
}

/// Physical memory: (total, available), bytes.
#[cfg(not(windows))]
#[must_use]
pub fn physical() -> Option<(u64, u64)> {
    None
}

/// A job object holding one step's processes (closed when dropped; its processes are not killed then).
#[derive(Debug)]
pub struct Job {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

// SAFETY: a job handle is a kernel handle, usable from any thread
#[cfg(windows)]
#[allow(unsafe_code)]
unsafe impl Send for Job {}

impl Job {
    /// A job with the child in it (None when the system has none or refuses).
    #[cfg(windows)]
    #[allow(unsafe_code)]
    #[must_use]
    pub fn of(child: &Child) -> Option<Self> {
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW};
        // SAFETY: no security attributes, no name
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return None;
        }
        let job = Self { handle };
        // SAFETY: both handles are open (the child's until it is dropped)
        let ok = unsafe { AssignProcessToJobObject(job.handle, child.as_raw_handle()) };
        (ok != 0).then_some(job)
    }

    /// A job with the child in it.
    #[cfg(not(windows))]
    #[must_use]
    pub fn of(_child: &Child) -> Option<Self> {
        None
    }

    /// The peak commit of all the job's processes together, bytes.
    #[cfg(windows)]
    #[allow(unsafe_code)]
    #[must_use]
    pub fn peak(&self) -> Option<u64> {
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, QueryInformationJobObject,
        };
        // SAFETY: a plain-data structure, all zeros is valid
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).ok()?;
        // SAFETY: info is the structure the class asks for, of that size
        let ok = unsafe {
            QueryInformationJobObject(
                self.handle,
                JobObjectExtendedLimitInformation,
                (&raw mut info).cast(),
                size,
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(info.PeakJobMemoryUsed as u64)
    }

    /// The peak commit of all the job's processes together, bytes.
    #[cfg(not(windows))]
    #[must_use]
    pub fn peak(&self) -> Option<u64> {
        None
    }
}

#[cfg(windows)]
impl Drop for Job {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: the handle is open and owned here
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

/// The child's own peak working set, bytes (as `rebuild_all` printed it before round OR7).
#[cfg(windows)]
#[allow(unsafe_code)]
#[must_use]
pub fn peak_working_set(child: &Child) -> Option<u64> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    // SAFETY: a plain-data structure, all zeros is valid
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    let size = u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS>()).ok()?;
    c.cb = size;
    // SAFETY: the child's handle is open; c is the structure of that size
    let ok = unsafe { GetProcessMemoryInfo(child.as_raw_handle(), &raw mut c, size) };
    (ok != 0).then_some(c.PeakWorkingSetSize as u64)
}

/// The child's own peak working set, bytes.
#[cfg(not(windows))]
#[must_use]
pub fn peak_working_set(_child: &Child) -> Option<u64> {
    None
}
