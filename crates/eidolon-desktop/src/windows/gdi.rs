//! GDI BitBlt primary-display capture → BMP.
//!
//! wraps: windows 0.62 Win32_Graphics_Gdi

use super::bmp::write_bmp32;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::mem::size_of;
use std::path::Path;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

pub(crate) fn capture_bmp(path: &Path) -> Result<()> {
    let (width, height) = screen_size()?;

    // SAFETY: classic GDI screen capture; all handles released on every path.
    unsafe {
        let hdc_screen = GetDC(Some(HWND::default()));
        if hdc_screen.is_invalid() {
            return Err(PhenoError::Platform("GetDC(NULL) failed".into()));
        }

        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        if hdc_mem.is_invalid() {
            ReleaseDC(Some(HWND::default()), hdc_screen);
            return Err(PhenoError::Platform("CreateCompatibleDC failed".into()));
        }

        let hbmp = CreateCompatibleBitmap(hdc_screen, width, height);
        if hbmp.is_invalid() {
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(Some(HWND::default()), hdc_screen);
            return Err(PhenoError::Platform("CreateCompatibleBitmap failed".into()));
        }

        let old = SelectObject(hdc_mem, HGDIOBJ(hbmp.0));
        if let Err(e) = BitBlt(hdc_mem, 0, 0, width, height, Some(hdc_screen), 0, 0, SRCCOPY) {
            SelectObject(hdc_mem, old);
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(Some(HWND::default()), hdc_screen);
            return Err(PhenoError::Platform(format!("BitBlt failed: {e}")));
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| PhenoError::Platform("screenshot size overflow".into()))?;
        let mut pixels = vec![0u8; pixel_count.saturating_mul(4)];

        let lines = GetDIBits(
            hdc_mem,
            hbmp,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        SelectObject(hdc_mem, old);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(Some(HWND::default()), hdc_screen);

        if lines == 0 {
            return Err(PhenoError::Platform("GetDIBits failed".into()));
        }

        write_bmp32(path, width as u32, height as u32, &pixels)?;
    }

    Ok(())
}

fn screen_size() -> Result<(i32, i32)> {
    // SAFETY: GetSystemMetrics is a pure query of desktop metrics.
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    if width <= 0 || height <= 0 {
        return Err(PhenoError::Platform(format!(
            "GetSystemMetrics returned invalid size {width}x{height}"
        )));
    }
    Ok((width, height))
}
