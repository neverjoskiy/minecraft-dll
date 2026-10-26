use std::ffi::CStr;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::{IMAGE_NT_HEADERS64, IMAGE_SECTION_HEADER, WriteProcessMemory};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE, PAGE_READWRITE, VirtualAllocEx,
    VirtualFreeEx, VirtualProtectEx,
};
use windows::Win32::System::SystemServices::{
    IMAGE_BASE_RELOCATION, IMAGE_DOS_HEADER, IMAGE_IMPORT_BY_NAME, IMAGE_IMPORT_DESCRIPTOR,
};
use windows::Win32::System::Threading::{
    CreateRemoteThread, WaitForSingleObject,
};

pub fn inject(pid: u32, dll_path: &str) -> Result<(), String> {
    let data = std::fs::read(dll_path).map_err(|e| e.to_string())?;
    let handle = crate::process::open_process(pid)?;
    let result = unsafe { inject_inner(handle, &data) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    result
}

unsafe fn inject_inner(handle: HANDLE, data: &[u8]) -> Result<(), String> {
    if data.len() < std::mem::size_of::<IMAGE_DOS_HEADER>() {
        return Err("invalid file".to_string());
    }
    let dos = &*(data.as_ptr() as *const IMAGE_DOS_HEADER);
    if dos.e_magic != 0x5A4D {
        return Err("invalid dos header".to_string());
    }
    let nt_offset = dos.e_lfanew as usize;
    if nt_offset + std::mem::size_of::<IMAGE_NT_HEADERS64>() > data.len() {
        return Err("invalid nt header offset".to_string());
    }
    let nt = &*(data.as_ptr().add(nt_offset) as *const IMAGE_NT_HEADERS64);
    if nt.Signature != 0x00004550 {
        return Err("invalid pe signature".to_string());
    }
    if nt.FileHeader.Machine != 0x8664 {
        return Err("not x64 dll".to_string());
    }
    if nt.OptionalHeader.Magic != 0x20B {
        return Err("not pe32+".to_string());
    }

    let image_size = nt.OptionalHeader.SizeOfImage as usize;
    let preferred_base = nt.OptionalHeader.ImageBase as usize;
    let entry_rva = nt.OptionalHeader.AddressOfEntryPoint as usize;
    let headers_size = nt.OptionalHeader.SizeOfHeaders as usize;

    let mut image = vec![0u8; image_size];
    let copy_len = headers_size.min(data.len()).min(image_size);
    image[..copy_len].copy_from_slice(&data[..copy_len]);

    let section_table_offset = nt_offset + std::mem::size_of::<IMAGE_NT_HEADERS64>();
    let sections = nt.FileHeader.NumberOfSections as usize;
    for i in 0..sections {
        let sec_ptr = data
            .as_ptr()
            .add(section_table_offset + i * std::mem::size_of::<IMAGE_SECTION_HEADER>())
            as *const IMAGE_SECTION_HEADER;
        let sec = &*sec_ptr;
        let va = sec.VirtualAddress as usize;
        let raw_size = sec.SizeOfRawData as usize;
        let raw_ptr = sec.PointerToRawData as usize;
        if raw_size == 0 || va >= image_size {
            continue;
        }
        let copy = raw_size.min(image_size - va);
        if raw_ptr + copy <= data.len() {
            image[va..va + copy].copy_from_slice(&data[raw_ptr..raw_ptr + copy]);
        }
    }

    let reloc_dir = &nt.OptionalHeader.DataDirectory[5];
    let reloc_rva = reloc_dir.VirtualAddress as usize;
    let reloc_size = reloc_dir.Size as usize;
    let actual_base_u64: u64;

    let remote_mem = VirtualAllocEx(
        handle,
        Some(preferred_base as *const _),
        image_size,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    );
    let remote_base = if remote_mem.is_null() {
        VirtualAllocEx(handle, None, image_size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE)
    } else {
        remote_mem
    };
    if remote_base.is_null() {
        return Err("VirtualAllocEx failed".to_string());
    }
    actual_base_u64 = remote_base as usize as u64;
    let delta = actual_base_u64 as isize - preferred_base as isize;

    if delta != 0 && reloc_rva != 0 && reloc_size != 0 {
        let mut cursor = reloc_rva;
        let reloc_end = reloc_rva + reloc_size;
        while cursor < reloc_end {
            let block = &*(image.as_ptr().add(cursor) as *const IMAGE_BASE_RELOCATION);
            if block.SizeOfBlock == 0 {
                break;
            }
            let count = (block.SizeOfBlock as usize
                - std::mem::size_of::<IMAGE_BASE_RELOCATION>())
                / 2;
            let entries = image.as_ptr().add(cursor + std::mem::size_of::<IMAGE_BASE_RELOCATION>())
                as *const u16;
            for j in 0..count {
                let entry = *entries.add(j);
                let typ = (entry >> 12) as usize;
                let off = (entry & 0x0FFF) as usize;
                if typ == 0 {
                    continue;
                }
                let loc_rva = block.VirtualAddress as usize + off;
                if typ == 10 {
                    let slot = image.as_mut_ptr().add(loc_rva) as *mut u64;
                    *slot = (*slot as isize + delta) as u64;
                } else if typ == 3 {
                    let slot = image.as_mut_ptr().add(loc_rva) as *mut u32;
                    *slot = (*slot as isize + delta as isize) as u32;
                } else {
                    let _ = VirtualFreeEx(handle, remote_base, 0, MEM_RELEASE);
                    return Err("unsupported relocation type".to_string());
                }
            }
            cursor += block.SizeOfBlock as usize;
        }
    }

    let import_dir = &nt.OptionalHeader.DataDirectory[1];
    let import_rva = import_dir.VirtualAddress as usize;
    if import_rva != 0 {
        let mut desc_ptr = image.as_ptr().add(import_rva) as *const IMAGE_IMPORT_DESCRIPTOR;
        loop {
            let desc = &*desc_ptr;
            if desc.Name == 0 && desc.FirstThunk == 0 {
                break;
            }
            if desc.Name == 0 {
                break;
            }
            let name_ptr = image.as_ptr().add(desc.Name as usize) as *const i8;
            let dep_name = CStr::from_ptr(name_ptr)
                .to_str()
                .map_err(|_| "invalid import name".to_string())?;
            let dep_module = LoadLibraryA(windows::core::PCSTR(dep_name.as_ptr() as *const u8))
                .map_err(|e| e.to_string())?;

            let original_thunk = if desc.Anonymous.OriginalFirstThunk != 0 {
                desc.Anonymous.OriginalFirstThunk as usize
            } else {
                desc.FirstThunk as usize
            };
            let mut thunk_rva = original_thunk;
            let mut func_rva = desc.FirstThunk as usize;
            while thunk_rva != 0 {
                let thunk = *(image.as_ptr().add(thunk_rva) as *const usize);
                if thunk == 0 {
                    break;
                }
                let func_addr = if thunk & (1usize << 63) != 0 {
                    let ordinal = (thunk & 0xFFFF) as u16;
                    GetProcAddress(
                        dep_module,
                        windows::core::PCSTR(ordinal as usize as *const u8),
                    )
                    .ok_or_else(|| "import not found".to_string())?
                } else {
                    let import_by_name =
                        image.as_ptr().add(thunk as usize) as *const IMAGE_IMPORT_BY_NAME;
                    let func_name = CStr::from_ptr((*import_by_name).Name.as_ptr());
                    GetProcAddress(dep_module, windows::core::PCSTR(func_name.as_ptr() as *const u8))
                        .ok_or_else(|| "import not found".to_string())?
                };
                let slot = image.as_mut_ptr().add(func_rva) as *mut usize;
                *slot = func_addr as usize;
                thunk_rva += std::mem::size_of::<usize>();
                func_rva += std::mem::size_of::<usize>();
            }
            desc_ptr = desc_ptr.add(1);
        }
    }

    let mut written: usize = 0;
    if WriteProcessMemory(
        handle,
        remote_base,
        image.as_ptr() as *const _,
        image_size,
        Some(&mut written),
    )
    .is_err()
    {
        let _ = VirtualFreeEx(handle, remote_base, 0, MEM_RELEASE);
        return Err("WriteProcessMemory failed".to_string());
    }

    let mut old_protect = windows::Win32::System::Memory::PAGE_PROTECTION_FLAGS(0);
    let _ = VirtualProtectEx(
        handle,
        remote_base,
        image_size,
        PAGE_EXECUTE_READWRITE,
        &mut old_protect,
    );

    if entry_rva != 0 {
        let entry = remote_base.add(entry_rva);
        call_dll_main(handle, entry, remote_base)?;
    }

    Ok(())
}

unsafe fn call_dll_main(handle: HANDLE, entry: *mut core::ffi::c_void, base: *mut core::ffi::c_void) -> Result<(), String> {
    let stub_size = 64usize;
    let stub_mem = VirtualAllocEx(
        handle,
        None,
        stub_size,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_EXECUTE_READWRITE,
    );
    if stub_mem.is_null() {
        return Err("stub alloc failed".to_string());
    }
    let mut buf = vec![0u8; stub_size];
    let code: Vec<u8> = vec![
        0x4C, 0x8B, 0x09,
        0x48, 0x8B, 0x49, 0x08,
        0xBA, 0x01, 0x00, 0x00, 0x00,
        0x45, 0x31, 0xC0,
        0x41, 0xFF, 0xD1,
        0x31, 0xC0,
        0xC3,
    ];
    buf[..code.len()].copy_from_slice(&code);

    let mut written: usize = 0;
    if WriteProcessMemory(handle, stub_mem, buf.as_ptr() as *const _, buf.len(), Some(&mut written))
        .is_err()
    {
        let _ = VirtualFreeEx(handle, stub_mem, 0, MEM_RELEASE);
        return Err("stub write failed".to_string());
    }

    #[repr(C)]
    struct Params {
        entry: u64,
        base: u64,
    }
    let params = Params {
        entry: entry as usize as u64,
        base: base as usize as u64,
    };
    let params_mem = VirtualAllocEx(handle, None, std::mem::size_of::<Params>(), MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if params_mem.is_null() {
        let _ = VirtualFreeEx(handle, stub_mem, 0, MEM_RELEASE);
        return Err("params alloc failed".to_string());
    }
    let _ = WriteProcessMemory(handle, params_mem, &params as *const _ as *const _, std::mem::size_of::<Params>(), None);

    let thread = CreateRemoteThread(handle, None, 0, Some(std::mem::transmute(stub_mem)), Some(params_mem), 0, None)
        .map_err(|e| e.to_string())?;
    let _ = WaitForSingleObject(thread, 0xFFFFFFFF);
    let _ = CloseHandle(thread);
    let _ = VirtualFreeEx(handle, stub_mem, 0, MEM_RELEASE);
    let _ = VirtualFreeEx(handle, params_mem, 0, MEM_RELEASE);
    Ok(())
}
