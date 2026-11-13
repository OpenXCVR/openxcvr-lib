//! Mmap-capture transport for SFF-8636: lower page and Pages 00h-03h
//! loaded from files, serving reads from the captured bytes.

use openxcvr_core::{
    error::{IoError, ModuleError},
    transport::Transport,
};

/// In-memory transport over captured SFF-8636 memory images: the 128-byte
/// lower page plus the 128-byte upper pages 00h-03h, loaded once from files.
/// Reads are served from the captured bytes; writes fail — a capture is
/// immutable evidence, not a device.
pub struct Sff8636MmapTransport {
    lower: [u8; 128],
    page_00h: [u8; 128],
    page_01h: [u8; 128],
    page_02h: [u8; 128],
    page_03h: [u8; 128],
}

impl Sff8636MmapTransport {
    /// Load the lower and page 00h-03h captures. Every image must be exactly
    /// 128 bytes — a truncated or over-long capture fails closed rather than
    /// decoding a partial page.
    pub fn from_files(
        lower_path: impl AsRef<std::path::Path>,
        page_00h_path: impl AsRef<std::path::Path>,
        page_01h_path: impl AsRef<std::path::Path>,
        page_02h_path: impl AsRef<std::path::Path>,
        page_03h_path: impl AsRef<std::path::Path>,
    ) -> Result<Self, ModuleError> {
        Ok(Self {
            lower: read_exact(lower_path)?,
            page_00h: read_exact(page_00h_path)?,
            page_01h: read_exact(page_01h_path)?,
            page_02h: read_exact(page_02h_path)?,
            page_03h: read_exact(page_03h_path)?,
        })
    }
}

fn read_exact<const N: usize>(path: impl AsRef<std::path::Path>) -> Result<[u8; N], ModuleError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| {
        ModuleError::Io(IoError::with_source(format!("{}: {e}", path.display()), e))
    })?;
    if bytes.len() != N {
        return Err(ModuleError::Io(
            format!(
                "{}: expected {N} bytes, got {}",
                path.display(),
                bytes.len()
            )
            .into(),
        ));
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

impl Transport for Sff8636MmapTransport {
    fn read(
        &mut self,
        _slave_addr: u8,
        page: u8,
        _bank: u8,
        offset: u8,
        buf: &mut [u8],
    ) -> Result<(), ModuleError> {
        let src: &[u8] = match (page, offset) {
            (0x00, 0..=127) => &self.lower[offset as usize..],
            (0x00, 128..=255) => &self.page_00h[(offset as usize - 128)..],
            (0x01, 128..=255) => &self.page_01h[(offset as usize - 128)..],
            (0x02, 128..=255) => &self.page_02h[(offset as usize - 128)..],
            (0x03, 128..=255) => &self.page_03h[(offset as usize - 128)..],
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
        page: u8,
        _bank: u8,
        offset: u8,
        data: &[u8],
    ) -> Result<(), ModuleError> {
        // In-memory read-modify-write over the captured pages: an offline
        // capture file is the transport for offline control testing, so a
        // write mutates the capture image rather than hardware. Only the
        // Lower-memory control region is writable (bytes 0-127 of page
        // 0x00), matching the control surface this crate exposes.
        if page != 0x00 || offset as usize + data.len() > 128 {
            return Err(ModuleError::Io(format!(
                "Sff8636MmapTransport write outside lower control region: page 0x{page:02x} offset {offset} len {}",
                data.len()
            ).into()));
        }
        self.lower[offset as usize..offset as usize + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn probe(&mut self) -> Result<(), ModuleError> {
        Ok(())
    }
}
