use std::{
    io::{Error, ErrorKind},
    path::Path,
};

use openxcvr_core::{error::ModuleError, transport::Transport};

use crate::module::CmisTransport;

/// A memory-mapped test/offline transport over captured page files.
pub struct CmisMmapTransport {
    lower: [u8; 128],
    page_00h: [u8; 128],
    page_01h: [u8; 128],
    page_02h: [u8; 128],
    page_03h: [u8; 128],
    page_04h: [u8; 128],
    page_05h: [u8; 128],
    page_11h: Option<[u8; 128]>,
}

impl CmisMmapTransport {
    /// Load the lower memory and Pages 00h-05h from captured files
    /// (128 bytes each).
    pub fn from_files(
        lower_path: impl AsRef<Path>,
        page_00h_path: impl AsRef<Path>,
        page_01h_path: impl AsRef<Path>,
        page_02h_path: impl AsRef<Path>,
        page_03h_path: impl AsRef<Path>,
        page_04h_path: impl AsRef<Path>,
        page_05h_path: impl AsRef<Path>,
    ) -> Result<Self, Error> {
        Ok(Self {
            lower: read_exact(lower_path)?,
            page_00h: read_exact(page_00h_path)?,
            page_01h: read_exact(page_01h_path)?,
            page_02h: read_exact(page_02h_path)?,
            page_03h: read_exact(page_03h_path)?,
            page_04h: read_exact(page_04h_path)?,
            page_05h: read_exact(page_05h_path)?,
            page_11h: None,
        })
    }

    /// Attach the captured bank-0 Page 11h without changing the seven-file API.
    /// An absent capture remains unsupported rather than being zero-filled.
    pub fn with_page_11h(mut self, path: impl AsRef<Path>) -> Result<Self, Error> {
        self.page_11h = Some(read_exact(path)?);
        Ok(self)
    }
}

fn read_exact<const N: usize>(path: impl AsRef<Path>) -> Result<[u8; N], Error> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)?;
    if bytes.len() != N {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "{}: expected {N} bytes, got {}",
                path.display(),
                bytes.len()
            ),
        ));
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

impl Transport for CmisMmapTransport {
    fn read(
        &mut self,
        _slave_addr: u8,
        page: u8,
        bank: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError> {
        let src: &[u8] = match (page, offset) {
            (0x00, 0..=127) => &self.lower[offset as usize..],
            (0x00, 128..=255) => &self.page_00h[(offset as usize - 128)..],
            (0x01, 128..=255) => &self.page_01h[(offset as usize - 128)..],
            (0x02, 128..=255) => &self.page_02h[(offset as usize - 128)..],
            (0x03, 128..=255) => &self.page_03h[(offset as usize - 128)..],
            (0x04, 128..=255) => &self.page_04h[(offset as usize - 128)..],
            (0x05, 128..=255) => &self.page_05h[(offset as usize - 128)..],
            (0x11, 128..=255) if bank == 0 => {
                let page = self
                    .page_11h
                    .as_ref()
                    .ok_or_else(|| ModuleError::Io("Page 11h bank 0 capture is absent".into()))?;
                &page[(offset as usize - 128)..]
            }
            _ => {
                return Err(ModuleError::Io(
                    format!("unsupported page/offset: 0x{page:02x}/{offset}").into(),
                ))
            }
        };
        if buf.len() > src.len() {
            return Err(ModuleError::Io(
                format!(
                    "read overflow: page=0x{page:02x} offset={offset} len={}",
                    buf.len()
                )
                .into(),
            ));
        }
        buf.copy_from_slice(&src[..buf.len()]);
        Ok(())
    }

    fn write(
        &mut self,
        _slave_addr: u8,
        _page: u8,
        _bank: u8,
        _offset: u8,
        _data: &[u8],
    ) -> Result<(), ModuleError> {
        Err(ModuleError::Io("mmap transport is read-only".into()))
    }

    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}

impl CmisTransport for CmisMmapTransport {}
