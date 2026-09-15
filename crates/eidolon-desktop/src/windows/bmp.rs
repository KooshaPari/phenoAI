//! Windows BMP (BI_RGB 32-bpp BGRA) writer shared by GDI and DXGI capture paths.
//!
//! Hermetic: compiles on all hosts so unit tests do not need `target_os=windows`.

use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Write a top-down 32-bpp BGRA Windows BMP to `path`.
#[cfg_attr(
    not(all(target_os = "windows", feature = "desktop-windows")),
    allow(dead_code)
)]
pub(crate) fn write_bmp32(path: &Path, width: u32, height: u32, bgra: &[u8]) -> Result<()> {
    let row_stride = width.saturating_mul(4);
    let pixel_bytes = row_stride.saturating_mul(height);
    if bgra.len() < pixel_bytes as usize {
        return Err(PhenoError::Platform("BMP pixel buffer too small".into()));
    }

    let file_header_size = 14u32;
    let info_header_size = 40u32;
    let offset = file_header_size + info_header_size;
    let file_size = offset + pixel_bytes;

    let mut file = File::create(path).map_err(|e| {
        PhenoError::Platform(format!(
            "failed to create screenshot {}: {e}",
            path.display()
        ))
    })?;

    // BITMAPFILEHEADER
    file.write_all(b"BM").map_err(io_platform)?;
    file.write_all(&file_size.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0u16.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0u16.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&offset.to_le_bytes()).map_err(io_platform)?;

    // BITMAPINFOHEADER — negative biHeight = top-down DIB (matches GDI GetDIBits
    // / DXGI Desktop Duplication scan order; Linux X11 path does the same).
    file.write_all(&info_header_size.to_le_bytes())
        .map_err(io_platform)?;
    file.write_all(&(width as i32).to_le_bytes())
        .map_err(io_platform)?;
    let bi_height = -(height as i32);
    file.write_all(&bi_height.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&1u16.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&32u16.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0u32.to_le_bytes()).map_err(io_platform)?; // BI_RGB
    file.write_all(&pixel_bytes.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0i32.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0i32.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0u32.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0u32.to_le_bytes()).map_err(io_platform)?;

    file.write_all(&bgra[..pixel_bytes as usize])
        .map_err(io_platform)?;
    Ok(())
}

#[cfg_attr(
    not(all(target_os = "windows", feature = "desktop-windows")),
    allow(dead_code)
)]
fn io_platform(e: std::io::Error) -> PhenoError {
    PhenoError::Platform(format!("screenshot I/O failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn write_bmp32_rejects_short_buffer() {
        let dir = std::env::temp_dir().join("eidolon-bmp-short");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("short.bmp");
        let err = write_bmp32(&path, 2, 2, &[0u8; 4]).unwrap_err();
        assert!(matches!(err, PhenoError::Platform(_)));
    }

    #[test]
    fn write_bmp32_roundtrip_header() {
        let dir = std::env::temp_dir().join("eidolon-bmp-ok");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("ok.bmp");
        let pixels = vec![0u8; 2 * 2 * 4];
        write_bmp32(&path, 2, 2, &pixels).expect("write");
        let bytes = fs::read(&path).expect("read");
        assert_eq!(&bytes[0..2], b"BM");
        assert_eq!(u32::from_le_bytes(bytes[18..22].try_into().unwrap()), 2);
        let bi_height = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
        assert_eq!(bi_height, -2, "top-down BMP requires negative biHeight");
        let _ = fs::remove_file(&path);
    }
}
