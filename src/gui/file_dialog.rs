

use std::path::PathBuf;

use windows::{
    Win32::{
        System::Com::{
            CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
            CoUninitialize,
        },
        UI::{
            Shell::{
                Common::COMDLG_FILTERSPEC, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR,
                FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog,
                SIGDN_FILESYSPATH,
            },
            WindowsAndMessaging::GetForegroundWindow,
        },
    },
    core::{HRESULT, Interface, PCWSTR, w},
};


pub fn pick_avatar_folder() -> Option<PathBuf> {
    pick_path(true, None)
}


pub fn pick_avatar_file() -> Option<PathBuf> {
    pick_path(
        false,
        Some(&[
            COMDLG_FILTERSPEC {
                pszName: w!("Avatar ZIP / bbmodel"),
                pszSpec: w!("*.zip;*.bbmodel"),
            },
            COMDLG_FILTERSPEC {
                pszName: w!("All files"),
                pszSpec: w!("*.*"),
            },
        ]),
    )
}

fn pick_path(folder: bool, filters: Option<&[COMDLG_FILTERSPEC]>) -> Option<PathBuf> {
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let need_uninit = hr == HRESULT(0) || hr == HRESULT(0x0001_0000); 
        let result = pick_path_inner(folder, filters);
        if need_uninit {
            CoUninitialize();
        }
        result
    }
}

unsafe fn pick_path_inner(folder: bool, filters: Option<&[COMDLG_FILTERSPEC]>) -> Option<PathBuf> {
    let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;

    let mut options = FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR;
    if folder {
        options |= FOS_PICKFOLDERS;
    } else {
        options |= FOS_FILEMUSTEXIST;
    }
    dialog.SetOptions(options).ok()?;

    if let Some(filters) = filters {
        let _ = dialog.SetFileTypes(filters);
        let _ = dialog.SetFileTypeIndex(1);
    }

    let title = if folder {
        w!("Select avatar folder")
    } else {
        w!("Select avatar ZIP or bbmodel")
    };
    let _ = dialog.SetTitle(title);

    let hwnd = GetForegroundWindow();
    dialog.Show(Some(hwnd)).ok()?;

    let item = dialog.GetResult().ok()?;
    let psz = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let path = {
        let wide = psz.0;
        let mut len = 0usize;
        while *wide.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(wide, len);
        String::from_utf16_lossy(slice)
    };
    CoTaskMemFree(Some(psz.0 as *const _));
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}
