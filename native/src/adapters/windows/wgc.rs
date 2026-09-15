//! WgcCapture — Windows Graphics Capture adapter.
//!
//! Primary capture path on Windows: GPU-accelerated, works with DirectX/game windows.
//! Falls back to XcapCapture when WGC is unavailable or for monitor-level captures.
//!
//! The WGC implementation is gated on target_os = "windows" throughout.
//! The non-Windows stubs exist solely to satisfy the compiler when cross-compiling.

use crate::adapters::xcap::{encode_rgba_png_frame, XcapCapture};
use crate::domain::capture::{CaptureError, Frame};
use crate::ports::CapturePort;
use async_trait::async_trait;
use tracing::{instrument, warn};

pub struct WgcCapture {
    fallback: XcapCapture,
}

impl WgcCapture {
    pub fn new() -> Result<Self, CaptureError> {
        Ok(Self {
            fallback: XcapCapture::new(),
        })
    }
}

#[async_trait]
impl CapturePort for WgcCapture {
    #[instrument(name = "wgc.capture_display", skip(self))]
    async fn capture_display(&self, monitor: u32) -> Result<Frame, CaptureError> {
        // WGC is window-scoped; delegate display capture to xcap.
        self.fallback.capture_display(monitor).await
    }

    #[instrument(name = "wgc.capture_window", skip(self), fields(title = ?title))]
    async fn capture_window(&self, title: Option<&str>) -> Result<Frame, CaptureError> {
        if let Some(t) = title {
            let title_owned = t.to_string();
            // Run WGC entirely on a blocking thread so WinRT non-Send types
            // never cross an await boundary in the async context.
            let result = tokio::task::spawn_blocking(move || {
                // Create a one-shot tokio runtime for the inner async WGC call.
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| CaptureError::CaptureFailed(format!("runtime build: {e}")))?;
                rt.block_on(capture_wgc(&title_owned))
            })
            .await
            .map_err(|e| CaptureError::CaptureFailed(format!("spawn_blocking panic: {e}")))?;

            match result {
                Ok(frame) => return Ok(frame),
                Err(e) => warn!("WGC capture failed ({}), falling back to xcap", e),
            }
        }
        self.fallback.capture_window(title).await
    }
}

// ---------------------------------------------------------------------------
// Windows implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
async fn capture_wgc(title: &str) -> Result<Frame, CaptureError> {
    use windows::{
        core::HSTRING,
        Graphics::{
            Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem},
            DirectX::DirectXPixelFormat,
        },
        Win32::{
            Graphics::{
                Direct3D::D3D_DRIVER_TYPE_HARDWARE,
                Direct3D11::{
                    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    D3D11_SDK_VERSION,
                },
                Dxgi::IDXGIDevice,
            },
            System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice,
            UI::WindowsAndMessaging::FindWindowW,
        },
    };

    // FindWindowW in windows-rs 0.58 returns Result<HWND>.
    let title_wide = HSTRING::from(title);
    let hwnd = unsafe { FindWindowW(None, &title_wide) }
        .map_err(|_| CaptureError::WindowNotFound(title.to_string()))?;

    // Create D3D11 device with BGRA support (required for WGC).
    let mut d3d_device: Option<ID3D11Device> = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            windows::Win32::Foundation::HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d3d_device),
            None,
            None,
        )
        .map_err(|e| CaptureError::CaptureFailed(format!("D3D11CreateDevice: {e}")))?;
    }
    let d3d_device = d3d_device.ok_or_else(|| {
        CaptureError::CaptureFailed("D3D11CreateDevice returned None".to_string())
    })?;

    // In windows-rs 0.58, cast() requires the Interface trait in scope.
    use windows::{core::Interface, Graphics::DirectX::Direct3D11::IDirect3DDevice};
    let dxgi_device: IDXGIDevice = d3d_device
        .cast()
        .map_err(|e| CaptureError::CaptureFailed(format!("cast to IDXGIDevice: {e}")))?;
    let winrt_inspectable = unsafe {
        CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device).map_err(|e| {
            CaptureError::CaptureFailed(format!("CreateDirect3D11DeviceFromDXGIDevice: {e}"))
        })?
    };
    // Cast IInspectable → IDirect3DDevice (required by CreateFreeThreaded in 0.58).
    let winrt_device: IDirect3DDevice = winrt_inspectable
        .cast()
        .map_err(|e| CaptureError::CaptureFailed(format!("cast to IDirect3DDevice: {e}")))?;

    // In windows-rs 0.58, HWND-based capture uses IGraphicsCaptureItemInterop::CreateForWindow.
    use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
    let interop: IGraphicsCaptureItemInterop =
        windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(
            |e| CaptureError::CaptureFailed(format!("IGraphicsCaptureItemInterop factory: {e}")),
        )?;
    let capture_item: GraphicsCaptureItem = unsafe {
        interop
            .CreateForWindow(hwnd)
            .map_err(|e| CaptureError::CaptureFailed(format!("CreateForWindow: {e}")))?
    };

    let frame_size = capture_item
        .Size()
        .map_err(|e| CaptureError::CaptureFailed(format!("GraphicsCaptureItem::Size: {e}")))?;
    let width = frame_size.Width as u32;
    let height = frame_size.Height as u32;

    let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &winrt_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        frame_size,
    )
    .map_err(|e| CaptureError::CaptureFailed(format!("CreateFreeThreaded: {e}")))?;

    let session = frame_pool
        .CreateCaptureSession(&capture_item)
        .map_err(|e| CaptureError::CaptureFailed(format!("CreateCaptureSession: {e}")))?;

    // Synchronous channel — frame arrives on the WGC callback thread.
    let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<u8>, CaptureError>>();
    let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    let tx2 = tx.clone();

    frame_pool
        .FrameArrived(&windows::Foundation::TypedEventHandler::new(
            move |pool: windows::core::Ref<Direct3D11CaptureFramePool>, _| {
                let pool_ref: Option<&Direct3D11CaptureFramePool> = pool.as_ref();
                if let Some(pool) = pool_ref {
                    if let Ok(frame) = pool.TryGetNextFrame() {
                        let result = extract_frame_pixels(&frame, width, height);
                        if let Some(sender) = tx2.lock().unwrap().take() {
                            let _ = sender.send(result);
                        }
                    }
                }
                Ok(())
            },
        ))
        .map_err(|e| CaptureError::CaptureFailed(format!("FrameArrived: {e}")))?;

    session
        .StartCapture()
        .map_err(|e| CaptureError::CaptureFailed(format!("StartCapture: {e}")))?;

    let pixels = tokio::task::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| {
                CaptureError::CaptureFailed("WGC timed out waiting for frame".to_string())
            })?
    })
    .await
    .map_err(|e| CaptureError::CaptureFailed(format!("spawn_blocking join: {e}")))??;

    let _ = session.Close();
    let _ = frame_pool.Close();

    // WGC delivers BGRA; swap B↔R to get RGBA for the image crate.
    let mut rgba = pixels;
    for chunk in rgba.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    encode_rgba_png_frame(width, height, &rgba)
}

/// Read pixel data from a WGC frame via a D3D11 staging texture.
#[cfg(target_os = "windows")]
fn extract_frame_pixels(
    frame: &windows::Graphics::Capture::Direct3D11CaptureFrame,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, CaptureError> {
    use windows::{
        core::Interface,
        Win32::{
            Graphics::{
                Direct3D11::{
                    ID3D11Device, ID3D11Texture2D, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE,
                    D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
                },
                Dxgi::Common::DXGI_SAMPLE_DESC,
            },
            System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess,
        },
    };

    let surface = frame
        .Surface()
        .map_err(|e| CaptureError::CaptureFailed(format!("frame.Surface: {e}")))?;
    let access: IDirect3DDxgiInterfaceAccess = surface.cast().map_err(|e| {
        CaptureError::CaptureFailed(format!("cast IDirect3DDxgiInterfaceAccess: {e}"))
    })?;
    let src_tex: ID3D11Texture2D = unsafe {
        access
            .GetInterface()
            .map_err(|e| CaptureError::CaptureFailed(format!("GetInterface: {e}")))?
    };

    let mut src_desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { src_tex.GetDesc(&mut src_desc) };

    // windows-rs 0.58: GetDevice() takes no args, returns Result<T>.
    let device: ID3D11Device = unsafe {
        src_tex
            .GetDevice()
            .map_err(|e| CaptureError::CaptureFailed(format!("GetDevice: {e}")))?
    };

    // windows-rs 0.58: GetImmediateContext() takes no args, returns Result<T>.
    let ctx = unsafe {
        device
            .GetImmediateContext()
            .map_err(|e| CaptureError::CaptureFailed(format!("GetImmediateContext: {e}")))?
    };

    // windows-rs 0.58: D3D11_TEXTURE2D_DESC flags fields are u32.
    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: src_desc.Format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0u32,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0u32,
    };

    let mut staging_opt: Option<ID3D11Texture2D> = None;
    unsafe {
        device
            .CreateTexture2D(&staging_desc, None, Some(&mut staging_opt))
            .map_err(|e| CaptureError::CaptureFailed(format!("CreateTexture2D: {e}")))?;
    }
    let staging = staging_opt
        .ok_or_else(|| CaptureError::CaptureFailed("CreateTexture2D returned None".to_string()))?;

    unsafe { ctx.CopyResource(&staging, &src_tex) };

    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|e| CaptureError::CaptureFailed(format!("Map: {e}")))?
    };

    let row_pitch = mapped.RowPitch as usize;
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let src_ptr = mapped.pData as *const u8;
    for row in 0..height as usize {
        unsafe {
            std::ptr::copy_nonoverlapping(
                src_ptr.add(row * row_pitch),
                pixels.as_mut_ptr().add(row * width as usize * 4),
                width as usize * 4,
            );
        }
    }
    unsafe { ctx.Unmap(&staging, 0) };
    Ok(pixels)
}

/// Non-Windows stub.
#[cfg(not(target_os = "windows"))]
async fn capture_wgc(_title: &str) -> Result<Frame, CaptureError> {
    Err(CaptureError::CaptureFailed(
        "WGC is not available on this platform".to_string(),
    ))
}
