use std::path::Path;

use anyhow::{Result, anyhow};

type OodleDecompress = unsafe extern "system" fn(
    *const u8,
    i64,
    *mut u8,
    i64,
    i32,
    i32,
    i64,
    *mut u8,
    i64,
    *mut u8,
    *mut u8,
    *mut u8,
    i64,
    i32,
) -> i64;

pub const CANDIDATES: &[&str] = &[
    "oo2core_8_win64.dll",
    "oo2core_9_win64.dll",
    "oo2core_7_win64.dll",
    "oo2core_6_win64.dll",
    "oo2core_5_win64.dll",
];

pub struct Oodle {
    _lib: libloading::Library,
    decompress: OodleDecompress,
    pub file: String,
}

unsafe impl Send for Oodle {}
unsafe impl Sync for Oodle {}

impl Oodle {
    pub fn find(root: &Path) -> Result<Oodle> {
        for name in CANDIDATES {
            let path = root.join(name);
            if path.is_file() {
                return Oodle::load(&path);
            }
        }
        Err(anyhow!("no oo2core dll beside the game"))
    }

    pub fn load(path: &Path) -> Result<Oodle> {
        unsafe {
            let lib = libloading::Library::new(path)?;
            let symbol: libloading::Symbol<OodleDecompress> = lib.get(b"OodleLZ_Decompress\0")?;
            let decompress = *symbol;
            Ok(Oodle {
                _lib: lib,
                decompress,
                file: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
            })
        }
    }

    pub fn run(&self, src: &[u8], dst: &mut [u8]) -> Result<usize> {
        let written = unsafe {
            (self.decompress)(
                src.as_ptr(),
                src.len() as i64,
                dst.as_mut_ptr(),
                dst.len() as i64,
                1,
                0,
                0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                3,
            )
        };
        if written <= 0 {
            return Err(anyhow!("oodle refused a block of {} bytes", src.len()));
        }
        Ok(written as usize)
    }
}
