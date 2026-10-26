use std::time::Instant;
use windows::Win32::{
    Foundation::FILETIME,
    System::{
        SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
        Threading::GetSystemTimes,
    },
};

#[derive(Clone, Debug)]
pub(crate) struct SystemStats {
    pub(crate) cpu_percent: Option<f32>,
    pub(crate) ram_percent: Option<f32>,
    pub(crate) ram_used_gb: Option<f32>,
    pub(crate) ram_total_gb: Option<f32>,
    pub(crate) last_idle: u64,
    pub(crate) last_kernel: u64,
    pub(crate) last_user: u64,
    pub(crate) last_refresh: Instant,
}

impl SystemStats {
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        let mut stats = Self {
            cpu_percent: None,
            ram_percent: None,
            ram_used_gb: None,
            ram_total_gb: None,
            last_idle: 0,
            last_kernel: 0,
            last_user: 0,
            last_refresh: now,
        };
        stats.refresh(true);
        stats
    }

    pub(crate) fn refresh_if_needed(&mut self) {
        if self.last_refresh.elapsed().as_secs_f32() >= 1.0 {
            self.refresh(false);
        }
    }

    pub(crate) fn refresh(&mut self, first: bool) {
        self.last_refresh = Instant::now();
        unsafe {
            let mut mem = MEMORYSTATUSEX {
                dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
                ..Default::default()
            };
            if GlobalMemoryStatusEx(&mut mem).is_ok() {
                let total = mem.ullTotalPhys as f32 / 1_073_741_824.0;
                let avail = mem.ullAvailPhys as f32 / 1_073_741_824.0;
                self.ram_total_gb = Some(total);
                self.ram_used_gb = Some((total - avail).max(0.0));
                self.ram_percent = Some(mem.dwMemoryLoad as f32);
            }

            let mut idle = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_ok() {
                let idle_now = filetime_to_u64(idle);
                let kernel_now = filetime_to_u64(kernel);
                let user_now = filetime_to_u64(user);
                if !first {
                    let idle_delta = idle_now.saturating_sub(self.last_idle);
                    let kernel_delta = kernel_now.saturating_sub(self.last_kernel);
                    let user_delta = user_now.saturating_sub(self.last_user);
                    let total_delta = kernel_delta.saturating_add(user_delta);
                    if total_delta > 0 {
                        let busy = total_delta.saturating_sub(idle_delta) as f32;
                        self.cpu_percent =
                            Some((busy * 100.0 / total_delta as f32).clamp(0.0, 100.0));
                    }
                }
                self.last_idle = idle_now;
                self.last_kernel = kernel_now;
                self.last_user = user_now;
            }
        }
    }
}

fn filetime_to_u64(value: FILETIME) -> u64 {
    ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64
}
