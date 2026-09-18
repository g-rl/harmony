/// what a drag ended in. the caller wants to know whether the file was taken, so a drop that
/// went nowhere can be said plainly and the temp file cleaned up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    /// the file was dropped somewhere and the target accepted it
    Dropped,
    /// the drag was let go over nothing, or escape was pressed
    Cancelled,
    /// the platform has no way to start a drag, or ole refused
    Unsupported,
}

#[cfg(windows)]
pub use windows_impl::drag_files;

/// every other platform: there is no drag to start. the caller says so rather than pretending.
#[cfg(not(windows))]
pub fn drag_files(_paths: &[std::path::PathBuf]) -> DragOutcome {
    DragOutcome::Unsupported
}

#[cfg(windows)]
mod windows_impl {
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;

    use windows::Win32::Foundation::{
        DV_E_FORMATETC, DV_E_TYMED, E_NOTIMPL, HGLOBAL, OLE_E_ADVISENOTSUPPORTED, POINT, S_OK,
    };
    use windows::Win32::System::Com::{
        DATADIR_GET, DVASPECT_CONTENT, FORMATETC, IAdviseSink, IDataObject, IDataObject_Impl,
        IEnumFORMATETC, IEnumSTATDATA, STGMEDIUM, TYMED_HGLOBAL,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::{
        DROPEFFECT, DROPEFFECT_COPY, DoDragDrop, IDropSource, IDropSource_Impl,
    };
    use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::Shell::{DROPFILES, SHCreateStdEnumFmtEtc};
    use windows::core::{BOOL, Result as WinResult, implement};

    use super::DragOutcome;

    /// `CF_HDROP`. the constant is a plain clipboard format number rather than a registered
    /// one, and every host that takes a dropped file reads it.
    const CF_HDROP: u16 = 15;

    /// starts an ole drag carrying the files, and blocks until they are dropped or let go.
    ///
    /// there is no window handle in it: `DoDragDrop` takes the data object and the source and
    /// nothing else.
    ///
    /// **it has to be called on the ui thread**, and blocks it until the pointer comes up.
    /// ole reads the button state and the capture off the calling thread, so a drag started
    /// from a worker sees no button held, ends the moment it begins, and shows nothing at all
    /// - which is exactly what it did the first time this was written.
    ///
    /// winit has already put the event loop thread into ole for its own drop target, so this
    /// does not initialize or uninitialize: tearing that down under it would take the
    /// window's ability to receive files with it.
    pub fn drag_files(paths: &[PathBuf]) -> DragOutcome {
        let Some(medium) = hdrop_bytes(paths) else {
            return DragOutcome::Unsupported;
        };
        // winit holds the capture from the press that started the carry, and ole wants it for
        // itself. without this the drag begins under a window that is still eating the moves.
        unsafe {
            let _ = ReleaseCapture();
        };
        let data: IDataObject = FileDrop { bytes: medium }.into();
        let source: IDropSource = DragSource.into();
        let mut effect = DROPEFFECT::default();
        // `DoDragDrop` answers `DRAGDROP_S_DROP` when the file was taken and
        // `DRAGDROP_S_CANCEL` when it was not. both are successes as far as the call goes -
        // the drop effect is what says whether anything actually happened.
        let result = unsafe { DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect) };
        if result.is_err() {
            return DragOutcome::Unsupported;
        }
        if effect == DROPEFFECT::default() {
            DragOutcome::Cancelled
        } else {
            DragOutcome::Dropped
        }
    }

    /// the one format on offer, as both the enumerator and the match test describe it
    fn hdrop_format() -> FORMATETC {
        FORMATETC {
            cfFormat: CF_HDROP,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        }
    }

    /// the `CF_HDROP` payload: a `DROPFILES` header, then every path as wide characters with a
    /// nul after each, then one more ending the list. it is the same block explorer puts on the
    /// clipboard for copied files.
    fn hdrop_bytes(paths: &[PathBuf]) -> Option<Vec<u8>> {
        let mut wide: Vec<u16> = Vec::new();
        for path in paths {
            let unit: Vec<u16> = path.as_os_str().encode_wide().collect();
            if unit.is_empty() {
                continue;
            }
            wide.extend(unit);
            wide.push(0);
        }
        if wide.is_empty() {
            return None;
        }
        wide.push(0);
        let header = size_of::<DROPFILES>();
        let mut bytes = vec![0u8; header + wide.len() * 2];
        let drop = DROPFILES {
            pFiles: header as u32,
            pt: POINT { x: 0, y: 0 },
            fNC: BOOL(0),
            fWide: BOOL(1),
        };
        // the header is written by hand rather than through a cast of the whole buffer: the
        // vector is byte aligned and `DROPFILES` is not
        let raw =
            unsafe { std::slice::from_raw_parts((&drop as *const DROPFILES).cast::<u8>(), header) };
        bytes[..header].copy_from_slice(raw);
        for (index, unit) in wide.iter().enumerate() {
            let at = header + index * 2;
            bytes[at..at + 2].copy_from_slice(&unit.to_le_bytes());
        }
        Some(bytes)
    }

    /// the smallest data object that will do: one format, one medium, read only. a host that
    /// asks for anything else is told the format is not there rather than being handed
    /// something it did not ask for.
    #[implement(IDataObject)]
    struct FileDrop {
        bytes: Vec<u8>,
    }

    impl FileDrop {
        fn wanted(format: *const FORMATETC) -> bool {
            if format.is_null() {
                return false;
            }
            let format = unsafe { &*format };
            format.cfFormat == CF_HDROP
                && format.dwAspect == DVASPECT_CONTENT.0
                && format.tymed & TYMED_HGLOBAL.0 as u32 != 0
        }

        fn medium(&self) -> WinResult<STGMEDIUM> {
            unsafe {
                let handle: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, self.bytes.len())?;
                let target = GlobalLock(handle);
                if target.is_null() {
                    return Err(E_NOTIMPL.into());
                }
                std::ptr::copy_nonoverlapping(
                    self.bytes.as_ptr(),
                    target.cast::<u8>(),
                    self.bytes.len(),
                );
                let _ = GlobalUnlock(handle);
                Ok(STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    u: windows::Win32::System::Com::STGMEDIUM_0 { hGlobal: handle },
                    pUnkForRelease: std::mem::ManuallyDrop::new(None),
                })
            }
        }
    }

    #[allow(non_snake_case)]
    impl IDataObject_Impl for FileDrop_Impl {
        fn GetData(&self, format: *const FORMATETC) -> WinResult<STGMEDIUM> {
            if !FileDrop::wanted(format) {
                return Err(DV_E_FORMATETC.into());
            }
            self.medium()
        }

        fn GetDataHere(&self, _format: *const FORMATETC, _medium: *mut STGMEDIUM) -> WinResult<()> {
            Err(DV_E_TYMED.into())
        }

        fn QueryGetData(&self, format: *const FORMATETC) -> windows::core::HRESULT {
            if FileDrop::wanted(format) {
                S_OK
            } else {
                DV_E_FORMATETC
            }
        }

        fn GetCanonicalFormatEtc(
            &self,
            _format: *const FORMATETC,
            out: *mut FORMATETC,
        ) -> windows::core::HRESULT {
            if !out.is_null() {
                unsafe { (*out).ptd = std::ptr::null_mut() };
            }
            E_NOTIMPL
        }

        fn SetData(
            &self,
            _format: *const FORMATETC,
            _medium: *const STGMEDIUM,
            _release: BOOL,
        ) -> WinResult<()> {
            Err(E_NOTIMPL.into())
        }

        fn EnumFormatEtc(&self, direction: u32) -> WinResult<IEnumFORMATETC> {
            // plenty of hosts ask what is on offer rather than asking after one format by
            // name, and a source that refuses to say hands them nothing they are willing to
            // accept. only the read direction is answered: nothing sets data on this object.
            if direction != DATADIR_GET.0 as u32 {
                return Err(E_NOTIMPL.into());
            }
            unsafe { SHCreateStdEnumFmtEtc(&[hdrop_format()]) }
        }

        fn DAdvise(
            &self,
            _format: *const FORMATETC,
            _flags: u32,
            _sink: windows::core::Ref<'_, IAdviseSink>,
        ) -> WinResult<u32> {
            Err(OLE_E_ADVISENOTSUPPORTED.into())
        }

        fn DUnadvise(&self, _token: u32) -> WinResult<()> {
            Err(OLE_E_ADVISENOTSUPPORTED.into())
        }

        fn EnumDAdvise(&self) -> WinResult<IEnumSTATDATA> {
            Err(OLE_E_ADVISENOTSUPPORTED.into())
        }
    }

    /// the gesture itself: keep going while the button is down, drop when it comes up, give
    /// up if escape is pressed or another button joins in.
    #[implement(IDropSource)]
    struct DragSource;

    #[allow(non_snake_case)]
    impl IDropSource_Impl for DragSource_Impl {
        fn QueryContinueDrag(
            &self,
            escape: BOOL,
            keys: MODIFIERKEYS_FLAGS,
        ) -> windows::core::HRESULT {
            const DRAGDROP_S_DROP: windows::core::HRESULT = windows::core::HRESULT(0x0004_0100);
            const DRAGDROP_S_CANCEL: windows::core::HRESULT = windows::core::HRESULT(0x0004_0101);
            if escape.as_bool() {
                return DRAGDROP_S_CANCEL;
            }
            if keys.0 & MK_LBUTTON.0 == 0 {
                return DRAGDROP_S_DROP;
            }
            S_OK
        }

        fn GiveFeedback(&self, _effect: DROPEFFECT) -> windows::core::HRESULT {
            // the shell draws the cursor. `DRAGDROP_S_USEDEFAULTCURSORS` is the way to say so.
            windows::core::HRESULT(0x0004_0102)
        }
    }
}

/// where a dragged export is written before it is handed over. `CF_HDROP` passes a path
/// rather than bytes, so the file has to exist on disk by the time the drag starts.
pub fn scratch_dir() -> std::path::PathBuf {
    crate::hm::storage::temp_dir().join("harmony-dragout")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scratch_dir_is_under_temp_and_named_for_the_app() {
        let dir = scratch_dir();
        assert!(dir.starts_with(crate::hm::storage::temp_dir()));
        assert!(dir.ends_with("harmony-dragout"));
    }

    #[cfg(not(windows))]
    #[test]
    fn a_platform_with_no_ole_says_so() {
        assert_eq!(
            drag_files(&[std::path::PathBuf::from("x.wav")]),
            DragOutcome::Unsupported
        );
    }
}

/// Clear out anything left in the scratch folder by earlier sessions. Files
/// that were dropped somewhere have been copied by the target; files that were
/// not are of no use to anybody.
pub fn sweep() {
    let dir = scratch_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
}
