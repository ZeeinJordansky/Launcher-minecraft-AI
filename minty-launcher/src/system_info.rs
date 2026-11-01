use std::thread;

#[derive(Debug, Clone, Copy)]
pub struct HardwareInfo {
    pub logical_cores: usize,
    pub total_memory_gb: u64,
    pub recommended_memory_gb: u8,
    pub low_end: bool,
}

pub fn detect() -> HardwareInfo {
    let logical_cores = thread::available_parallelism().map(|value| value.get()).unwrap_or(2);
    let total_memory_gb = total_memory_gb().max(2);
    HardwareInfo {
        logical_cores,
        total_memory_gb,
        recommended_memory_gb: recommend_memory(total_memory_gb),
        low_end: total_memory_gb <= 8 || logical_cores <= 4,
    }
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