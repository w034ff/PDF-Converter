//! The physical memory installed in the machine, and the memory the worker itself is using
//! (design §5.3).
//!
//! The main process sizes its decoding budget from the installed amount. It is the installed
//! amount, not the free amount, because the free amount differs from run to run.

/// Returns the installed physical memory in bytes, or `None` if the OS does not say.
///
/// Reads `MemTotal` of `/proc/meminfo` on Linux and `GlobalMemoryStatusEx` on Windows; other
/// operating systems always give `None`.
pub fn installed_memory() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
    }
    #[cfg(windows)]
    {
        windows_installed_memory()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

/// Returns what the worker's own memory limit counts, in bytes, or `None` if the OS does not say
/// (design organize §1, "元の PDF を `lopdf` で読む").
///
/// The limit is on the address space on Linux (`RLIMIT_AS`), so this reads `VmSize` of
/// `/proc/self/status`. It is on the committed memory of the process on Windows (the job's
/// process memory limit), so this reads `PrivateUsage` from `K32GetProcessMemoryInfo`. Other
/// operating systems always give `None`.
pub fn current_usage() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        parse_vm_size(&std::fs::read_to_string("/proc/self/status").ok()?)
    }
    #[cfg(windows)]
    {
        windows_current_usage()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

/// Finds the `MemTotal:` line of the contents of `/proc/meminfo` and returns its value in bytes.
///
/// The kernel always writes the value in kB; any other unit, a missing number or a value that
/// overflows is a form this does not know, so it gives `None`.
#[cfg(any(target_os = "linux", test))]
fn parse_meminfo(contents: &str) -> Option<u64> {
    parse_kilobytes(contents, "MemTotal:")
}

/// Finds the `VmSize:` line of the contents of `/proc/self/status`; same rules as
/// [`parse_meminfo`].
#[cfg(any(target_os = "linux", test))]
fn parse_vm_size(contents: &str) -> Option<u64> {
    parse_kilobytes(contents, "VmSize:")
}

/// The value in bytes of the line of `contents` that starts with `label`.
#[cfg(any(target_os = "linux", test))]
fn parse_kilobytes(contents: &str, label: &str) -> Option<u64> {
    let value = contents.lines().find_map(|line| line.strip_prefix(label))?;
    let mut fields = value.split_whitespace();
    let kilobytes: u64 = fields.next()?.parse().ok()?;
    if fields.next()? != "kB" || fields.next().is_some() {
        return None;
    }
    kilobytes.checked_mul(1024)
}

// `GlobalMemoryStatusEx` fills a struct through a pointer, and no maintained safe wrapper is
// worth a new dependency for this one read (design §5.3 gives the reason).
#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_installed_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    // SAFETY: an all-zero MEMORYSTATUSEX is a valid value of plain integers; `dwLength` is set
    // below, as the API requires, before the struct is passed.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = u32::try_from(std::mem::size_of::<MEMORYSTATUSEX>()).ok()?;

    // SAFETY: `status` is an initialised struct with `dwLength` set that outlives the call,
    // and the call only writes into it.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok == 0 {
        return None;
    }
    Some(status.ullTotalPhys)
}

// Same reason as `windows_installed_memory`: the call fills a struct through a pointer.
#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_current_usage() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: an all-zero PROCESS_MEMORY_COUNTERS_EX is a valid value of plain integers; `cb` is
    // set below, as the API requires, before the struct is passed.
    let mut counters: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    let size = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>()).ok()?;
    counters.cb = size;

    // SAFETY: `GetCurrentProcess` returns a pseudo handle that needs no closing. `counters` is
    // an initialised struct with `cb` set that outlives the call, the API takes the longer
    // PROCESS_MEMORY_COUNTERS_EX through a pointer to its first part, and the call only writes
    // into it.
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            std::ptr::from_mut(&mut counters).cast(),
            size,
        )
    };
    if ok == 0 {
        return None;
    }
    u64::try_from(counters.PrivateUsage).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn installed_memory_is_known_and_positive() {
        let installed = installed_memory().expect("Linux and Windows report installed memory");
        assert!(installed > 0);
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn current_usage_is_known_and_positive() {
        let usage = current_usage().expect("Linux and Windows report the worker's usage");
        assert!(usage > 0);
    }

    #[test]
    fn parses_vm_size_among_the_other_fields() {
        let status = "Name:\tworker\nVmPeak:\t  900 kB\nVmSize:\t  123456 kB\nVmSizeX:\t 9 kB\n";
        assert_eq!(parse_vm_size(status), Some(123_456 * 1024));
    }

    #[test]
    fn rejects_a_vm_size_it_does_not_know() {
        for status in [
            "",
            "VmPeak: 1 kB\n",
            "VmSize:\n",
            "VmSize: kB\n",
            "VmSize: abc kB\n",
            "VmSize: 1024\n",
            "VmSize: 1024 MB\n",
            "VmSize: 1024 kB extra\n",
            "VmSize: 18446744073709551615 kB\n",
        ] {
            assert_eq!(parse_vm_size(status), None, "{status:?}");
        }
    }

    #[test]
    fn parses_mem_total_in_kilobytes() {
        let meminfo = "MemTotal:       16384000 kB\nMemFree:         1234567 kB\n";
        assert_eq!(parse_meminfo(meminfo), Some(16_384_000 * 1024));
    }

    #[test]
    fn finds_mem_total_after_other_lines() {
        let meminfo = "SwapTotal: 1 kB\nMemTotal: 2048 kB\nMemTotalExtra: 9 kB\n";
        assert_eq!(parse_meminfo(meminfo), Some(2048 * 1024));
    }

    #[test]
    fn rejects_a_form_it_does_not_know() {
        for meminfo in [
            "",
            "MemFree: 1 kB\n",
            "MemTotal:\n",
            "MemTotal: kB\n",
            "MemTotal: abc kB\n",
            "MemTotal: -5 kB\n",
            "MemTotal: 1024\n",
            "MemTotal: 1024 MB\n",
            "MemTotal: 1024 kB extra\n",
            "MemTotal: 18446744073709551615 kB\n",
        ] {
            assert_eq!(parse_meminfo(meminfo), None, "{meminfo:?}");
        }
    }
}
