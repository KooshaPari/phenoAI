// SPDX-License-Identifier: MIT OR Apache-2.0
//! Rust bindings for the PhenoCompose NVMS C API.

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicU64, Ordering};

pub mod sys {
    use std::os::raw::{c_char, c_int, c_ulonglong};

    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum NvmsTier {
        Wasm = 1,
        Gvisor = 2,
        Firecracker = 3,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum NvmsStatus {
        Stopped = 0,
        Starting = 1,
        Running = 2,
        Stopping = 3,
        Error = 4,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum NvmsGpuBackend {
        None = 0,
        AppleMetal = 1,
        NvidiaCuda = 2,
        AmdRocm = 3,
        IntelOneApi = 4,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum NvmsMemoryType {
        Cpu = 0,
        Gpu = 1,
        Unified = 2,
    }

    #[repr(C)]
    pub struct NvmsInstance {
        pub id: c_ulonglong,
        pub tier: NvmsTier,
        pub status: NvmsStatus,
        pub name: *mut c_char,
        pub gpu_backend: NvmsGpuBackend,
        pub memory_type: NvmsMemoryType,
        pub gpu_memory_bytes: c_ulonglong,
    }

    #[repr(C)]
    pub struct NvmsGpuDevice {
        pub name: [c_char; 256],
        pub backend: NvmsGpuBackend,
        pub memory_bytes: c_ulonglong,
        pub compute_units: c_int,
        pub supports_unified_memory: bool,
    }

    #[repr(C)]
    pub struct NvmsPerfStats {
        pub startup_time_ns: c_ulonglong,
        pub memory_used_bytes: c_ulonglong,
        pub gpu_utilization: f64,
    }

    unsafe extern "C" {
        pub fn nvms_version() -> *const c_char;
        pub fn nvms_platform_info() -> *const c_char;
        pub fn nvms_init() -> c_int;
        pub fn nvms_init_gpu(backend: NvmsGpuBackend) -> c_int;
        pub fn nvms_gpu_info() -> NvmsGpuDevice;
        pub fn nvms_supports_gpu() -> bool;
        pub fn nvms_supports_unified_memory() -> bool;
        pub fn nvms_apple_silicon_init() -> c_int;
        pub fn nvms_apple_ane_available() -> bool;
        pub fn nvms_apple_unified_memory_alloc(size: c_ulonglong) -> *mut std::ffi::c_void;
        pub fn nvms_cuda_init() -> c_int;
        pub fn nvms_cuda_device_count() -> c_int;
        pub fn nvms_cuda_alloc_unified(size: c_ulonglong) -> *mut std::ffi::c_void;
        pub fn nvms_rocm_init() -> c_int;
        pub fn nvms_rocm_device_count() -> c_int;
        pub fn nvms_neon_available() -> bool;
        pub fn nvms_instance_create(tier: NvmsTier, name: *const c_char) -> *mut NvmsInstance;
        pub fn nvms_instance_destroy(inst: *mut NvmsInstance) -> c_int;
        pub fn nvms_instance_start(inst: *mut NvmsInstance) -> c_int;
        pub fn nvms_instance_stop(inst: *mut NvmsInstance) -> c_int;
        pub fn nvms_instance_status(inst: *mut NvmsInstance) -> NvmsStatus;
        pub fn nvms_perf_stats() -> NvmsPerfStats;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Wasm = 1,
    Gvisor = 2,
    Firecracker = 3,
}

impl From<Tier> for sys::NvmsTier {
    fn from(value: Tier) -> Self {
        match value {
            Tier::Wasm => sys::NvmsTier::Wasm,
            Tier::Gvisor => sys::NvmsTier::Gvisor,
            Tier::Firecracker => sys::NvmsTier::Firecracker,
        }
    }
}

impl From<sys::NvmsTier> for Tier {
    fn from(value: sys::NvmsTier) -> Self {
        match value {
            sys::NvmsTier::Wasm => Tier::Wasm,
            sys::NvmsTier::Gvisor => Tier::Gvisor,
            sys::NvmsTier::Firecracker => Tier::Firecracker,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

impl From<sys::NvmsStatus> for Status {
    fn from(value: sys::NvmsStatus) -> Self {
        match value {
            sys::NvmsStatus::Stopped => Status::Stopped,
            sys::NvmsStatus::Starting => Status::Starting,
            sys::NvmsStatus::Running => Status::Running,
            sys::NvmsStatus::Stopping => Status::Stopping,
            sys::NvmsStatus::Error => Status::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    None,
    AppleMetal,
    NvidiaCuda,
    AmdRocm,
    IntelOneApi,
}

impl From<GpuBackend> for sys::NvmsGpuBackend {
    fn from(value: GpuBackend) -> Self {
        match value {
            GpuBackend::None => sys::NvmsGpuBackend::None,
            GpuBackend::AppleMetal => sys::NvmsGpuBackend::AppleMetal,
            GpuBackend::NvidiaCuda => sys::NvmsGpuBackend::NvidiaCuda,
            GpuBackend::AmdRocm => sys::NvmsGpuBackend::AmdRocm,
            GpuBackend::IntelOneApi => sys::NvmsGpuBackend::IntelOneApi,
        }
    }
}

impl From<sys::NvmsGpuBackend> for GpuBackend {
    fn from(value: sys::NvmsGpuBackend) -> Self {
        match value {
            sys::NvmsGpuBackend::None => GpuBackend::None,
            sys::NvmsGpuBackend::AppleMetal => GpuBackend::AppleMetal,
            sys::NvmsGpuBackend::NvidiaCuda => GpuBackend::NvidiaCuda,
            sys::NvmsGpuBackend::AmdRocm => GpuBackend::AmdRocm,
            sys::NvmsGpuBackend::IntelOneApi => GpuBackend::IntelOneApi,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    Cpu,
    Gpu,
    Unified,
}

impl From<sys::NvmsMemoryType> for MemoryType {
    fn from(value: sys::NvmsMemoryType) -> Self {
        match value {
            sys::NvmsMemoryType::Cpu => MemoryType::Cpu,
            sys::NvmsMemoryType::Gpu => MemoryType::Gpu,
            sys::NvmsMemoryType::Unified => MemoryType::Unified,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GpuDevice {
    pub name: String,
    pub backend: GpuBackend,
    pub memory_bytes: u64,
    pub compute_units: u32,
    pub supports_unified_memory: bool,
}

#[derive(Debug, Clone)]
pub struct PerfStats {
    pub startup_time_ns: u64,
    pub memory_used_bytes: u64,
    pub gpu_utilization: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmsError {
    InitFailed,
    CreateFailed,
    StartFailed,
    StopFailed,
    DestroyFailed,
    AppleSiliconNotSupported,
    CudaInitFailed,
    RocmInitFailed,
    /// Catch-all for unknown non-zero return codes from the C library.
    /// The contained `i32` is the raw error code.
    Other(i32),
}

impl NvmsError {
    /// Get the raw error code that this variant corresponds to.
    /// `Other(code)` returns the wrapped code unchanged.
    pub fn raw_code(&self) -> i32 {
        match *self {
            Self::InitFailed => -1,
            Self::CreateFailed => -2,
            Self::StartFailed => -3,
            Self::StopFailed => -4,
            Self::DestroyFailed => -5,
            Self::AppleSiliconNotSupported => -100,
            Self::CudaInitFailed => -200,
            Self::RocmInitFailed => -201,
            Self::Other(c) => c,
        }
    }
}

impl std::fmt::Display for NvmsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitFailed => write!(f, "NVMS init failed"),
            Self::CreateFailed => write!(f, "NVMS instance creation failed"),
            Self::StartFailed => write!(f, "NVMS instance start failed"),
            Self::StopFailed => write!(f, "NVMS instance stop failed"),
            Self::DestroyFailed => write!(f, "NVMS instance destroy failed"),
            Self::AppleSiliconNotSupported => write!(f, "Apple Silicon not supported by this build"),
            Self::CudaInitFailed => write!(f, "CUDA initialization failed"),
            Self::RocmInitFailed => write!(f, "ROCm initialization failed"),
            Self::Other(c) => write!(f, "NVMS returned unknown error code: {}", c),
        }
    }
}

impl std::error::Error for NvmsError {}

impl From<i32> for NvmsError {
    fn from(code: i32) -> Self {
        match code {
            -1 => Self::InitFailed,
            -2 => Self::CreateFailed,
            -3 => Self::StartFailed,
            -4 => Self::StopFailed,
            -5 => Self::DestroyFailed,
            -100 => Self::AppleSiliconNotSupported,
            -200 => Self::CudaInitFailed,
            -201 => Self::RocmInitFailed,
            other => Self::Other(other),
        }
    }
}

pub struct Instance {
    ptr: *mut sys::NvmsInstance,
}

impl Instance {
    /// # Safety
    /// The underlying C API owns the returned pointer and it must remain valid.
    pub unsafe fn create(tier: Tier, name: &str) -> Option<Self> {
        let name = CString::new(name).ok()?;
        let ptr = sys::nvms_instance_create(tier.into(), name.as_ptr());
        (!ptr.is_null()).then_some(Self { ptr })
    }

    pub fn start(&self) -> Result<(), NvmsError> {
        match unsafe { sys::nvms_instance_start(self.ptr) } {
            0 => Ok(()),
            _ => Err(NvmsError::StartFailed),
        }
    }

    pub fn stop(&self) -> Result<(), NvmsError> {
        match unsafe { sys::nvms_instance_stop(self.ptr) } {
            0 => Ok(()),
            _ => Err(NvmsError::StopFailed),
        }
    }

    pub fn status(&self) -> Status {
        unsafe { sys::nvms_instance_status(self.ptr).into() }
    }

    pub fn tier(&self) -> Tier {
        unsafe { (*self.ptr).tier.into() }
    }

    pub fn gpu_backend(&self) -> GpuBackend {
        unsafe { (*self.ptr).gpu_backend.into() }
    }

    pub fn memory_type(&self) -> MemoryType {
        unsafe { (*self.ptr).memory_type.into() }
    }

    pub fn id(&self) -> u64 {
        unsafe { (*self.ptr).id }
    }

    pub fn name(&self) -> String {
        unsafe { cstr_to_string((*self.ptr).name) }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = unsafe { sys::nvms_instance_destroy(self.ptr) };
    }
}

pub fn version() -> String {
    unsafe { cstr_to_string(sys::nvms_version()) }
}

pub fn platform_info() -> String {
    unsafe { cstr_to_string(sys::nvms_platform_info()) }
}

pub fn init() -> Result<(), NvmsError> {
    match unsafe { sys::nvms_init() } {
        0 => Ok(()),
        _ => Err(NvmsError::InitFailed),
    }
}

pub fn init_gpu(backend: GpuBackend) -> Result<(), NvmsError> {
    match unsafe { sys::nvms_init_gpu(backend.into()) } {
        0 => Ok(()),
        _ => Err(NvmsError::InitFailed),
    }
}

pub fn gpu_info() -> GpuDevice {
    unsafe {
        let device = sys::nvms_gpu_info();
        GpuDevice {
            name: cstr_to_string(device.name.as_ptr()),
            backend: device.backend.into(),
            memory_bytes: device.memory_bytes,
            compute_units: device.compute_units as u32,
            supports_unified_memory: device.supports_unified_memory,
        }
    }
}

pub fn supports_gpu() -> bool {
    unsafe { sys::nvms_supports_gpu() }
}

pub fn supports_unified_memory() -> bool {
    unsafe { sys::nvms_supports_unified_memory() }
}

pub fn apple_silicon_init() -> Result<(), NvmsError> {
    match unsafe { sys::nvms_apple_silicon_init() } {
        0 => Ok(()),
        _ => Err(NvmsError::AppleSiliconNotSupported),
    }
}

pub fn apple_ane_available() -> bool {
    unsafe { sys::nvms_apple_ane_available() }
}

pub fn apple_unified_memory_alloc(size: u64) -> *mut c_void {
    unsafe { sys::nvms_apple_unified_memory_alloc(size) }
}

pub fn cuda_init() -> Result<(), NvmsError> {
    match unsafe { sys::nvms_cuda_init() } {
        0 => Ok(()),
        _ => Err(NvmsError::CudaInitFailed),
    }
}

pub fn cuda_device_count() -> i32 {
    unsafe { sys::nvms_cuda_device_count() }
}

pub fn cuda_alloc_unified(size: u64) -> *mut c_void {
    unsafe { sys::nvms_cuda_alloc_unified(size) }
}

pub fn rocm_init() -> Result<(), NvmsError> {
    match unsafe { sys::nvms_rocm_init() } {
        0 => Ok(()),
        _ => Err(NvmsError::RocmInitFailed),
    }
}

pub fn rocm_device_count() -> i32 {
    unsafe { sys::nvms_rocm_device_count() }
}

pub fn neon_available() -> bool {
    unsafe { sys::nvms_neon_available() }
}

pub fn perf_stats() -> PerfStats {
    unsafe {
        let stats = sys::nvms_perf_stats();
        PerfStats {
            startup_time_ns: stats.startup_time_ns,
            memory_used_bytes: stats.memory_used_bytes,
            gpu_utilization: stats.gpu_utilization,
        }
    }
}

/// List all tracked instances with their basic info.
///
/// Returns a vector of `(id, tier, status, name)` tuples for every
/// instance that has been created (via FFI) and not yet destroyed.
/// The list is a snapshot; concurrent destroy operations may make
/// individual entries stale.
pub fn list_instances() -> Vec<(u64, Tier, Status, String)> {
    crate::shim::INSTANCES
        .lock()
        .map(|guard| {
            guard
                .iter()
                .map(|(id, tier, status, name)| (*id, Tier::from(*tier), Status::from(*status), name.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}

fn write_name(dst: &mut [c_char; 256], value: &str) {
    let bytes = value.as_bytes();
    let len = bytes.len().min(dst.len().saturating_sub(1));
    for (index, byte) in bytes.iter().copied().take(len).enumerate() {
        dst[index] = byte as c_char;
    }
    dst[len] = 0;
}

#[cfg(not(nvms_real_ffi))]
mod shim {
    use super::*;
    use std::sync::Mutex;

    static INSTANCE_COUNTER: AtomicU64 = AtomicU64::new(0);
    pub(crate) static INSTANCES: Mutex<Vec<(u64, sys::NvmsTier, sys::NvmsStatus, String)>> = Mutex::new(Vec::new());

    fn current_backend() -> sys::NvmsGpuBackend {
        if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
            sys::NvmsGpuBackend::AppleMetal
        } else {
            sys::NvmsGpuBackend::None
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_version() -> *const c_char {
        c"1.0.0-rust-bindings".as_ptr()
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_platform_info() -> *const c_char {
        if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
            c"darwin/arm64".as_ptr()
        } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
            c"linux/amd64".as_ptr()
        } else {
            c"unknown/unknown".as_ptr()
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_init() -> i32 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_init_gpu(_backend: sys::NvmsGpuBackend) -> i32 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_gpu_info() -> sys::NvmsGpuDevice {
        let mut device = sys::NvmsGpuDevice {
            name: [0; 256],
            backend: current_backend(),
            memory_bytes: 8 * 1024 * 1024 * 1024,
            compute_units: 8,
            supports_unified_memory: cfg!(target_os = "macos") && cfg!(target_arch = "aarch64"),
        };
        write_name(&mut device.name, "NVMS Reference Device");
        device
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_supports_gpu() -> bool {
        current_backend() != sys::NvmsGpuBackend::None
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_supports_unified_memory() -> bool {
        cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_apple_silicon_init() -> i32 {
        if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
            0
        } else {
            -1
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_apple_ane_available() -> bool {
        cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_apple_unified_memory_alloc(size: u64) -> *mut c_void {
        let mut buf = Vec::<u8>::with_capacity(size as usize);
        let ptr = buf.as_mut_ptr();
        std::mem::forget(buf);
        ptr.cast()
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_cuda_init() -> i32 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_cuda_device_count() -> i32 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_cuda_alloc_unified(size: u64) -> *mut c_void {
        nvms_apple_unified_memory_alloc(size)
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_rocm_init() -> i32 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_rocm_device_count() -> i32 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_neon_available() -> bool {
        cfg!(target_arch = "aarch64")
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_instance_create(tier: sys::NvmsTier, name: *const c_char) -> *mut sys::NvmsInstance {
        let id = INSTANCE_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
        let display_name = if name.is_null() {
            "unnamed".to_string()
        } else {
            unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned()
        };
        let owned_name = CString::new(display_name.as_str()).expect("valid CString");
        let instance = sys::NvmsInstance {
            id,
            tier,
            status: sys::NvmsStatus::Running,
            name: owned_name.into_raw(),
            gpu_backend: current_backend(),
            memory_type: if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
                sys::NvmsMemoryType::Unified
            } else {
                sys::NvmsMemoryType::Cpu
            },
            gpu_memory_bytes: 0,
        };
        let ptr = Box::into_raw(Box::new(instance));
        if let Ok(mut guard) = INSTANCES.lock() {
            guard.push((id, tier, sys::NvmsStatus::Running, display_name));
        }
        ptr
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_instance_destroy(inst: *mut sys::NvmsInstance) -> i32 {
        if inst.is_null() {
            return -1;
        }
        let id = unsafe { (*inst).id };
        if let Ok(mut guard) = INSTANCES.lock() {
            guard.retain(|(i, _, _, _)| *i != id);
        }
        unsafe {
            let instance = Box::from_raw(inst);
            if !instance.name.is_null() {
                let _ = CString::from_raw(instance.name);
            }
        }
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_instance_start(inst: *mut sys::NvmsInstance) -> i32 {
        if inst.is_null() {
            return -1;
        }
        unsafe {
            (*inst).status = sys::NvmsStatus::Running;
        }
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_instance_stop(inst: *mut sys::NvmsInstance) -> i32 {
        if inst.is_null() {
            return -1;
        }
        unsafe {
            (*inst).status = sys::NvmsStatus::Stopped;
        }
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_instance_status(inst: *mut sys::NvmsInstance) -> sys::NvmsStatus {
        if inst.is_null() {
            return sys::NvmsStatus::Error;
        }
        unsafe { (*inst).status }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn nvms_perf_stats() -> sys::NvmsPerfStats {
        sys::NvmsPerfStats {
            startup_time_ns: 1_000_000,
            memory_used_bytes: 64 * 1024 * 1024,
            gpu_utilization: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_version_and_platform() {
        assert!(version().starts_with("1.0"));
        assert!(platform_info().contains('/'));
    }

    #[test]
    fn drives_instance_lifecycle() {
        init().unwrap();
        let instance = unsafe { Instance::create(Tier::Wasm, "test") }.unwrap();
        assert_eq!(instance.tier(), Tier::Wasm);
        assert_eq!(instance.status(), Status::Running);
        instance.stop().unwrap();
        assert_eq!(instance.status(), Status::Stopped);
        instance.start().unwrap();
        assert_eq!(instance.status(), Status::Running);
    }
}
