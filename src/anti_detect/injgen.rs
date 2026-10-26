use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows::Win32::System::Memory::{
    MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_GUARD, PAGE_NOACCESS, PAGE_READWRITE, VirtualQuery,
};

use super::jvm_image::{self, JvmImageRange};



#[allow(dead_code)]
fn signature_a() -> u32 {
    
    (0x08u32 << 16) | 0x06
}

#[allow(dead_code)]
fn signature_b() -> u32 {
    
    (0xFCE0u32 << 16) | 0x1E99
}

#[allow(dead_code)]
static LAST_SCRUB_MS: AtomicU64 = AtomicU64::new(0);







pub fn neutralize() -> NeutralizeReport {
    NeutralizeReport::default()
}


pub fn neutralize_periodic(_min_interval: Duration) -> Option<NeutralizeReport> {
    None
}

#[allow(dead_code)]
fn neutralize_legacy_unsafe() -> NeutralizeReport {
    let Some(image) = jvm_image::jvm_image_range() else {
        return NeutralizeReport::default();
    };

    let mut hits_a = Vec::new();
    let mut hits_b = Vec::new();
    walk_rw_dwords(image, |addr, value| {
        if value == signature_a() {
            hits_a.push(addr);
        } else if value == signature_b() {
            hits_b.push(addr);
        }
    });

    if hits_a.is_empty() || hits_b.is_empty() {
        return NeutralizeReport {
            signature_a_hits: hits_a.len(),
            signature_b_hits: hits_b.len(),
            patched: 0,
        };
    }

    let mut patched = 0usize;
    for addr in hits_b.iter().copied().chain(hits_a.iter().copied()) {
        if scrub_dword(addr) {
            patched += 1;
        }
    }

    NeutralizeReport {
        signature_a_hits: hits_a.len(),
        signature_b_hits: hits_b.len(),
        patched,
    }
}

#[allow(dead_code)]
fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NeutralizeReport {
    pub signature_a_hits: usize,
    pub signature_b_hits: usize,
    pub patched: usize,
}

#[allow(dead_code)]
fn walk_rw_dwords(image: JvmImageRange, mut on_dword: impl FnMut(*mut u32, u32)) {
    let mut address = image.base as usize;
    let end = image.base as usize + image.size;

    while address < end {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let queried = unsafe {
            VirtualQuery(
                Some(address as *const _),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if queried == 0 {
            break;
        }

        let region_base = mbi.BaseAddress as usize;
        let region_size = mbi.RegionSize;
        let region_end = region_base.saturating_add(region_size);
        let scan_start = address.max(region_base);
        let scan_end = end.min(region_end);

        let readable = mbi.State == MEM_COMMIT
            && (mbi.Protect.0 & PAGE_READWRITE.0) != 0
            && (mbi.Protect.0 & (PAGE_GUARD.0 | PAGE_NOACCESS.0)) == 0;

        if readable && scan_start < scan_end {
            let len = scan_end - scan_start;
            let aligned = len & !3;
            if aligned >= 4 {
                let slice = unsafe { std::slice::from_raw_parts(scan_start as *const u8, aligned) };
                for offset in (0..aligned).step_by(4) {
                    let value = u32::from_le_bytes([
                        slice[offset],
                        slice[offset + 1],
                        slice[offset + 2],
                        slice[offset + 3],
                    ]);
                    let ptr = (scan_start + offset) as *mut u32;
                    on_dword(ptr, value);
                }
            }
        }

        if region_end <= address {
            address = address.saturating_add(0x1000);
        } else {
            address = region_end;
        }
    }
}

#[allow(dead_code)]
fn scrub_dword(addr: *mut u32) -> bool {
    if addr.is_null() {
        return false;
    }

    unsafe {
        std::ptr::write_volatile(addr, 0);
    }
    true
}
