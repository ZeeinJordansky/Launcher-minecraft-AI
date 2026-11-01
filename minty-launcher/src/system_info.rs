use std::thread;

#[derive(Debug, Clone)]
pub struct HardwareInfo {
    pub cpu_name: String,
    pub logical_cores: usize,
    pub total_memory_gb: u64,
    pub recommended_memory_gb: u8,
    pub low_end: bool,
}

pub fn detect() -> HardwareInfo {
    let logical_cores = thread::available_parallelism().map(|value| value.get()).unwrap_or(2);
    let total_memory_gb = total_memory_gb().max(2);
    HardwareInfo {
        cpu_name: cpu_name(),
        logical_cores,
        total_memory_gb,
        recommended_memory_gb: recommend_memory(total_memory_gb),
        low_end: total_memory_gb <= 8 || logical_cores <= 4,
    }
}

#[cfg(windows)]
fn cpu_name() -> String {
    use std::{ffi::c_void, ptr::null_mut};
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(key: *mut c_void, sub_key: *const u16, options: u32, access: u32, result: *mut *mut c_void) -> i32;
        fn RegQueryValueExW(key: *mut c_void, value: *const u16, reserved: *mut u32, kind: *mut u32, data: *mut u8, size: *mut u32) -> i32;
        fn RegCloseKey(key: *mut c_void) -> i32;
    }
    let root = 0x80000002u32 as usize as *mut c_void; // HKEY_LOCAL_MACHINE
    let sub_key: Vec<u16> = "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0"
        .encode_utf16().chain(std::iter::once(0)).collect();
    let value_name: Vec<u16> = "ProcessorNameString".encode_utf16().chain(std::iter::once(0)).collect();
    let mut key = null_mut();
    let opened = unsafe { RegOpenKeyExW(root, sub_key.as_ptr(), 0, 0x20019, &mut key) };
    if opened == 0 {
        let mut buffer = vec![0u16; 256];
        let mut bytes = (buffer.len() * std::mem::size_of::<u16>()) as u32;
        let read = unsafe {
            RegQueryValueExW(key, value_name.as_ptr(), null_mut(), null_mut(),
                buffer.as_mut_ptr() as *mut u8, &mut bytes)
        };
        unsafe { RegCloseKey(key); }
        if read == 0 {
            let length = (bytes as usize / 2).saturating_sub(1).min(buffer.len());
            let name = String::from_utf16_lossy(&buffer[..length]).trim().to_owned();
            if !name.is_empty() { return name; }
        }
    }
    std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "Процессор не определён".to_owned())
}

#[cfg(not(windows))]
fn cpu_name() -> String {
    std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "Процессор не определён".to_owned())
}

pub fn recommend_memory(total_gb: u64) -> u8 {
    match total_gb {
        0..=4 => 2,
        5..=7 => 3,
        8..=11 => 4,
        12..=15 => 6,
        16..=23 => 8,
        _ => (total_gb / 2).min(12) as u8,
    }
}

#[cfg(windows)]
fn total_memory_gb() -> u64 {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }
    let mut status = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 { return 8; }
    status.total_phys / (1024 * 1024 * 1024)
}

#[cfg(not(windows))]
fn total_memory_gb() -> u64 {
    #[cfg(target_os = "linux")]
    if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
        if let Some(value) = contents.lines().find(|line| line.starts_with("MemTotal:"))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse::<u64>().ok())
        {
            return value / 1024 / 1024;
        }
    }
    8
}

#[cfg(test)]
mod tests {
    use super::recommend_memory;
    #[test]
    fn memory_recommendation_is_conservative() {
        assert_eq!(recommend_memory(4), 2);
        assert_eq!(recommend_memory(8), 4);
        assert_eq!(recommend_memory(16), 8);
        assert_eq!(recommend_memory(32), 12);
    }
}