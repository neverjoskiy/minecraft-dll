use std::{ffi::OsStr, fs, os::windows::ffi::OsStrExt, path::PathBuf, sync::OnceLock};

use windows::{
    Win32::Media::MediaFoundation::{
        IMFSourceReader, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
        MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
        MF_SOURCE_READERF_ENDOFSTREAM, MF_VERSION, MFCreateAttributes, MFCreateMediaType,
        MFCreateSourceReaderFromURL, MFMediaType_Video, MFSTARTUP_LITE, MFStartup,
        MFVideoFormat_RGB32,
    },
    core::PCWSTR,
};

const BANNER_BYTES: &[u8] = include_bytes!("../resources/profile_banner.mp4");
const TARGET_WIDTH: u32 = 960;
const TARGET_HEIGHT: u32 = 540;

pub(crate) struct DecodedFrame {
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
}

pub(crate) struct Mp4BannerDecoder {
    path: PathBuf,
    reader: IMFSourceReader,
    width: usize,
    height: usize,
}

impl Mp4BannerDecoder {
    pub(crate) fn new() -> Result<Self, String> {
        ensure_media_foundation()?;
        let path = ensure_banner_file()?;
        let mut decoder = Self {
            path,
            reader: create_reader(PathBuf::new())?,
            width: 0,
            height: 0,
        };
        decoder.reopen()?;
        Ok(decoder)
    }

    pub(crate) fn next_frame(&mut self) -> Option<DecodedFrame> {
        match self.read_frame() {
            Ok(Some(frame)) => Some(frame),
            Ok(None) => {
                let _ = self.reopen();
                self.read_frame().ok().flatten()
            }
            Err(_) => {
                let _ = self.reopen();
                None
            }
        }
    }

    fn reopen(&mut self) -> Result<(), String> {
        self.reader = create_reader(self.path.clone())?;
        let media_type = unsafe { MFCreateMediaType() }.map_err(|err| err.message().to_string())?;
        unsafe {
            media_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|err| err.message().to_string())?;
            media_type
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(|err| err.message().to_string())?;
            media_type
                .SetUINT64(
                    &MF_MT_FRAME_SIZE,
                    ((TARGET_WIDTH as u64) << 32) | TARGET_HEIGHT as u64,
                )
                .map_err(|err| err.message().to_string())?;
            self.reader
                .SetCurrentMediaType(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    None,
                    &media_type,
                )
                .map_err(|err| err.message().to_string())?;
            let current = self
                .reader
                .GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
                .map_err(|err| err.message().to_string())?;
            let packed = current
                .GetUINT64(&MF_MT_FRAME_SIZE)
                .map_err(|err| err.message().to_string())?;
            self.width = (packed >> 32) as usize;
            self.height = (packed & 0xffff_ffff) as usize;
        }
        if self.width == 0 || self.height == 0 {
            return Err("video has empty frame size".to_string());
        }
        Ok(())
    }

    fn read_frame(&mut self) -> Result<Option<DecodedFrame>, String> {
        for _ in 0..8 {
            let mut flags = 0u32;
            let mut sample = None;
            unsafe {
                self.reader
                    .ReadSample(
                        MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                        0,
                        None,
                        Some(&mut flags),
                        None,
                        Some(&mut sample),
                    )
                    .map_err(|err| err.message().to_string())?;
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(None);
            }
            let Some(sample) = sample else {
                continue;
            };
            let buffer = unsafe { sample.ConvertToContiguousBuffer() }
                .map_err(|err| err.message().to_string())?;
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            unsafe {
                buffer
                    .Lock(&mut ptr, None, Some(&mut len))
                    .map_err(|err| err.message().to_string())?;
            }
            let result = if ptr.is_null() || len == 0 {
                None
            } else {
                let src = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
                Some(bgra_to_rgba(src, self.width, self.height))
            };
            let _ = unsafe { buffer.Unlock() };
            if let Some(rgba) = result {
                return Ok(Some(DecodedFrame {
                    size: [self.width, self.height],
                    rgba,
                }));
            }
        }
        Ok(None)
    }
}

fn bgra_to_rgba(src: &[u8], width: usize, height: usize) -> Vec<u8> {
    let pixels = width.saturating_mul(height);
    let mut out = Vec::with_capacity(pixels.saturating_mul(4));
    for px in src.chunks_exact(4).take(pixels) {
        out.push(px[2]);
        out.push(px[1]);
        out.push(px[0]);
        out.push(px[3]);
    }
    out
}

fn ensure_media_foundation() -> Result<(), String> {
    static STARTED: OnceLock<Result<(), String>> = OnceLock::new();
    STARTED
        .get_or_init(|| unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_LITE).map_err(|err| err.message().to_string())
        })
        .clone()
}

fn ensure_banner_file() -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join("xz_profile_banner.mp4");
    match fs::read(&path) {
        Ok(existing) if existing.len() == BANNER_BYTES.len() => return Ok(path),
        _ => {}
    }
    fs::write(&path, BANNER_BYTES).map_err(|err| err.to_string())?;
    Ok(path)
}

fn create_reader(path: PathBuf) -> Result<IMFSourceReader, String> {
    if path.as_os_str().is_empty() {
        ensure_media_foundation()?;
        let path = ensure_banner_file()?;
        return create_reader(path);
    }
    let wide = wide_null(path.as_os_str());
    let mut attrs = None;
    unsafe {
        MFCreateAttributes(&mut attrs, 1).map_err(|err| err.message().to_string())?;
    }
    let attrs = attrs.ok_or_else(|| "MFCreateAttributes returned None".to_string())?;
    unsafe {
        attrs
            .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
            .map_err(|err| err.message().to_string())?;
    }
    unsafe { MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), &attrs) }
        .map_err(|err| err.message().to_string())
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}
