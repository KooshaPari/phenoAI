//! DXGI Desktop Duplication primary-output capture → BMP.
//!
//! wraps: windows 0.62 — Win32_Graphics_Dxgi + Direct3D11
//! (https://crates.io/crates/windows)
//!
//! Flow: `CreateDXGIFactory1` → adapter/output → `D3D11CreateDevice` →
//! `IDXGIOutput1::DuplicateOutput` → `AcquireNextFrame` → staging
//! `CopyResource`/`Map` → BGRA BMP.

use super::bmp::write_bmp32;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::path::Path;
use std::ptr;
use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication,
    IDXGIResource, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
};

const ACQUIRE_TIMEOUT_MS: u32 = 500;
const ACQUIRE_ATTEMPTS: u32 = 10;

pub(crate) fn capture_bmp(path: &Path) -> Result<()> {
    // SAFETY: COM/DXGI objects are released via Drop (windows-rs RAII).
    unsafe { capture_bmp_inner(path) }
}

unsafe fn capture_bmp_inner(path: &Path) -> Result<()> {
    let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| {
        PhenoError::Platform(format!("CreateDXGIFactory1 failed: {e}"))
    })?;

    let (adapter, output) = primary_output(&factory)?;
    let output1: IDXGIOutput1 = output.cast().map_err(|e| {
        PhenoError::Platform(format!("IDXGIOutput1 cast failed: {e}"))
    })?;

    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    let feature_levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
    let mut feature_level = D3D_FEATURE_LEVEL::default();

    D3D11CreateDevice(
        &adapter,
        D3D_DRIVER_TYPE_UNKNOWN,
        HMODULE::default(),
        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        Some(&feature_levels),
        D3D11_SDK_VERSION,
        Some(&mut device),
        Some(&mut feature_level),
        Some(&mut context),
    )
    .map_err(|e| PhenoError::Platform(format!("D3D11CreateDevice failed: {e}")))?;

    let device = device.ok_or_else(|| PhenoError::Platform("D3D11 device null".into()))?;
    let context = context.ok_or_else(|| PhenoError::Platform("D3D11 context null".into()))?;

    let duplication: IDXGIOutputDuplication = output1.DuplicateOutput(&device).map_err(|e| {
        PhenoError::Platform(format!("DuplicateOutput failed: {e}"))
    })?;

    let (frame_tex, _frame_info) = acquire_frame(&duplication)?;
    let pixels = copy_texture_bgra(&device, &context, &frame_tex)?;
    let _ = duplication.ReleaseFrame();

    let mut desc = D3D11_TEXTURE2D_DESC::default();
    frame_tex.GetDesc(&mut desc);
    if desc.Width == 0 || desc.Height == 0 {
        return Err(PhenoError::Platform(
            "DXGI frame texture has zero dimensions".into(),
        ));
    }

    write_bmp32(path, desc.Width, desc.Height, &pixels)?;
    let _ = feature_level; // retained for future logging
    Ok(())
}

unsafe fn primary_output(
    factory: &IDXGIFactory1,
) -> Result<(windows::Win32::Graphics::Dxgi::IDXGIAdapter1, IDXGIOutput)> {
    for adapter_idx in 0u32..16 {
        let adapter = match factory.EnumAdapters1(adapter_idx) {
            Ok(a) => a,
            Err(_) => break,
        };
        for output_idx in 0u32..16 {
            let output = match adapter.EnumOutputs(output_idx) {
                Ok(o) => o,
                Err(_) => break,
            };
            let desc = output.GetDesc().map_err(|e| {
                PhenoError::Platform(format!("IDXGIOutput::GetDesc failed: {e}"))
            })?;
            if desc.AttachedToDesktop.as_bool() {
                return Ok((adapter, output));
            }
        }
    }
    Err(PhenoError::Platform(
        "no DXGI output attached to desktop".into(),
    ))
}

unsafe fn acquire_frame(
    duplication: &IDXGIOutputDuplication,
) -> Result<(ID3D11Texture2D, DXGI_OUTDUPL_FRAME_INFO)> {
    let mut last_err = String::from("AcquireNextFrame exhausted retries");

    for _ in 0..ACQUIRE_ATTEMPTS {
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        match duplication.AcquireNextFrame(ACQUIRE_TIMEOUT_MS, &mut frame_info, &mut resource) {
            Ok(()) => {
                let Some(resource) = resource else {
                    let _ = duplication.ReleaseFrame();
                    last_err = "AcquireNextFrame returned null resource".into();
                    continue;
                };
                // Empty update (pointer-only) — release and wait for a desktop present.
                if frame_info.LastPresentTime == 0 && frame_info.AccumulatedFrames == 0 {
                    let _ = duplication.ReleaseFrame();
                    last_err = "DXGI frame had no desktop present".into();
                    continue;
                }
                let texture: ID3D11Texture2D = resource.cast().map_err(|e| {
                    PhenoError::Platform(format!("IDXGIResource→ID3D11Texture2D cast failed: {e}"))
                })?;
                return Ok((texture, frame_info));
            }
            Err(e) => {
                let code = e.code();
                if code == DXGI_ERROR_WAIT_TIMEOUT {
                    last_err = format!("DXGI_ERROR_WAIT_TIMEOUT after {ACQUIRE_TIMEOUT_MS}ms");
                    continue;
                }
                if code == DXGI_ERROR_ACCESS_LOST {
                    return Err(PhenoError::Platform(format!(
                        "DXGI_ERROR_ACCESS_LOST: {e}"
                    )));
                }
                return Err(PhenoError::Platform(format!(
                    "AcquireNextFrame failed: {e}"
                )));
            }
        }
    }

    Err(PhenoError::Platform(last_err))
}

unsafe fn copy_texture_bgra(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    src: &ID3D11Texture2D,
) -> Result<Vec<u8>> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    src.GetDesc(&mut desc);

    // Staging texture for CPU readback (B8G8R8A8 preferred; accept src format).
    let format = if desc.Format == DXGI_FORMAT(0) {
        DXGI_FORMAT_B8G8R8A8_UNORM
    } else {
        desc.Format
    };

    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: desc.Width,
        Height: desc.Height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };

    let mut staging: Option<ID3D11Texture2D> = None;
    device
        .CreateTexture2D(&staging_desc, None, Some(&mut staging))
        .map_err(|e| PhenoError::Platform(format!("CreateTexture2D(staging) failed: {e}")))?;
    let staging =
        staging.ok_or_else(|| PhenoError::Platform("staging texture null".into()))?;

    context.CopyResource(&staging, src);

    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    context
        .Map(
            &staging,
            0,
            D3D11_MAP_READ,
            0,
            Some(&mut mapped as *mut _),
        )
        .map_err(|e| PhenoError::Platform(format!("ID3D11DeviceContext::Map failed: {e}")))?;

    let width = desc.Width as usize;
    let height = desc.Height as usize;
    let row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| PhenoError::Platform("DXGI row size overflow".into()))?;
    let mut pixels = vec![0u8; row_bytes.saturating_mul(height)];
    let pitch = mapped.RowPitch as usize;
    if mapped.pData.is_null() {
        context.Unmap(&staging, 0);
        return Err(PhenoError::Platform("mapped DXGI frame pData null".into()));
    }

    for y in 0..height {
        let src_row = (mapped.pData as *const u8).add(y * pitch);
        let dst_row = pixels[y * row_bytes..].as_mut_ptr();
        // RowPitch may exceed width*4 (GPU alignment).
        ptr::copy_nonoverlapping(src_row, dst_row, row_bytes.min(pitch));
    }

    context.Unmap(&staging, 0);
    Ok(pixels)
}
