use windows::{Win32::System::LibraryLoader::GetModuleHandleA, core::s};


#[derive(Clone, Copy, Debug)]
pub struct JvmImageRange {
    pub base: *mut u8,
    pub size: usize,
}


pub fn jvm_image_range() -> Option<JvmImageRange> {
    let module = unsafe { GetModuleHandleA(s!("jvm.dll")).ok()? };
    let base = module.0 as *mut u8;
    if base.is_null() {
        return None;
    }

    let size = unsafe { pe_size_of_image(base)? };
    if size == 0 {
        return None;
    }

    Some(JvmImageRange { base, size })
}

unsafe fn pe_size_of_image(base: *mut u8) -> Option<usize> {
    let dos = base as *const ImageDosHeader;
    if unsafe { (*dos).e_magic } != 0x5A4D {
        return None;
    }

    let nt_offset = unsafe { (*dos).e_lfanew } as usize;
    if nt_offset > 0x1000 {
        return None;
    }

    let nt = unsafe { base.add(nt_offset) } as *const ImageNtHeaders64;
    if unsafe { (*nt).signature } != 0x0000_4550 {
        return None;
    }

    let magic = unsafe { (*nt).optional_header.magic };
    let size = match magic {
        0x20B | 0x10B => unsafe { (*nt).optional_header.size_of_image },
        _ => return None,
    };

    Some(size as usize)
}

#[repr(C)]
struct ImageDosHeader {
    e_magic: u16,
    _pad: [u8; 58],
    e_lfanew: i32,
}

#[repr(C)]
struct ImageFileHeader {
    _machine: u16,
    _number_of_sections: u16,
    _time_date_stamp: u32,
    _pointer_to_symbol_table: u32,
    _number_of_symbols: u32,
    _size_of_optional_header: u16,
    _characteristics: u16,
}


#[repr(C)]
struct ImageOptionalHeaderShared {
    magic: u16,
    _pad: [u8; 54],
    size_of_image: u32,
}

#[repr(C)]
struct ImageNtHeaders64 {
    signature: u32,
    _file_header: ImageFileHeader,
    optional_header: ImageOptionalHeaderShared,
}
