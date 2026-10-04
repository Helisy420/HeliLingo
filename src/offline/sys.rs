//! Windows bits for the offline tiers: the graphics card (DXGI), CUDA and
//! Vulkan availability, physical cores, free disk space, a folder picker
//! and a kill-on-close Job object for the llama-server child.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
    DXGI_QUERY_VIDEO_MEMORY_INFO, IDXGIAdapter1, IDXGIAdapter3, IDXGIDevice, IDXGIFactory1,
};
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::core::{HSTRING, Interface, PCSTR, PCWSTR};

pub const NVIDIA: u32 = 0x10de;

/// The graphics card offline models can use.
#[derive(Clone, Debug, PartialEq)]
pub struct Gpu {
    pub name: String,
    pub vendor: u32,
    /// Dedicated video memory, bytes.
    pub vram: u64,
    /// Driver version as the vendor shows it ("591.86" for NVIDIA).
    pub driver: Option<String>,
    /// CUDA version the driver supports, e.g. (13, 0); NVIDIA only.
    pub cuda: Option<(u32, u32)>,
}

impl Gpu {
    pub fn vram_gb(&self) -> f32 {
        self.vram as f32 / (1u64 << 30) as f32
    }
}

/// The adapter with the most dedicated memory (cached: the hardware
/// doesn't change while the app runs).
pub fn gpu() -> Option<Gpu> {
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    GPU.get_or_init(detect_gpu).clone()
}

fn best_adapter() -> Option<IDXGIAdapter1> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut best: Option<(u64, IDXGIAdapter1)> = None;
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            let Ok(desc) = adapter.GetDesc1() else { continue };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let mem = desc.DedicatedVideoMemory as u64;
            if best.as_ref().is_none_or(|(m, _)| mem > *m) {
                best = Some((mem, adapter));
            }
        }
        best.filter(|(m, _)| *m >= 512 << 20).map(|(_, a)| a)
    }
}

fn detect_gpu() -> Option<Gpu> {
    let adapter = best_adapter()?;
    unsafe {
        let desc = adapter.GetDesc1().ok()?;
        let len = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..len]).trim().to_owned();
        let driver = adapter
            .CheckInterfaceSupport(&IDXGIDevice::IID)
            .ok()
            .map(|v| driver_version(desc.VendorId, v as u64));
        let cuda = if desc.VendorId == NVIDIA { cuda_version() } else { None };
        Some(Gpu { name, vendor: desc.VendorId, vram: desc.DedicatedVideoMemory as u64, driver, cuda })
    }
}

/// The UMD version (a.b.c.d packed in 16-bit parts) as the vendor shows
/// it: NVIDIA's "591.86" is the last five digits of c.d.
fn driver_version(vendor: u32, v: u64) -> String {
    let (a, b, c, d) = ((v >> 48) & 0xffff, (v >> 32) & 0xffff, (v >> 16) & 0xffff, v & 0xffff);
    if vendor == NVIDIA {
        let n = (c % 10) * 10000 + d;
        format!("{}.{:02}", n / 100, n % 100)
    } else {
        format!("{a}.{b}.{c}.{d}")
    }
}

/// `cuDriverGetVersion` from nvcuda.dll (no CUDA toolkit needed).
fn cuda_version() -> Option<(u32, u32)> {
    unsafe {
        let lib = LoadLibraryW(&HSTRING::from("nvcuda.dll")).ok()?;
        let f = GetProcAddress(lib, PCSTR(c"cuDriverGetVersion".as_ptr() as *const u8))?;
        let f: extern "system" fn(*mut i32) -> i32 = std::mem::transmute(f);
        let mut v = 0i32;
        (f(&mut v) == 0 && v > 0).then_some((v as u32 / 1000, (v as u32 % 1000) / 10))
    }
}

/// Free video memory right now: the OS budget minus what's in use.
pub fn free_vram() -> Option<u64> {
    let adapter: IDXGIAdapter3 = best_adapter()?.cast().ok()?;
    let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
    unsafe { adapter.QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info).ok()? };
    Some(info.Budget.saturating_sub(info.CurrentUsage))
}

/// Vulkan loader present (any vendor's driver installs it).
pub fn has_vulkan() -> bool {
    unsafe { LoadLibraryW(&HSTRING::from("vulkan-1.dll")).is_ok() }
}

/// Physical CPU cores (logical processors without SMT siblings).
pub fn physical_cores() -> usize {
    static CORES: OnceLock<usize> = OnceLock::new();
    *CORES.get_or_init(|| {
        use windows::Win32::System::SystemInformation::{
            GetLogicalProcessorInformation, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
        };
        let logical = std::thread::available_parallelism().map_or(4, |n| n.get());
        unsafe {
            let mut len = 0u32;
            let _ = GetLogicalProcessorInformation(None, &mut len);
            let n = len as usize / size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
            if n == 0 {
                return logical;
            }
            let mut buf = vec![SYSTEM_LOGICAL_PROCESSOR_INFORMATION::default(); n];
            if GetLogicalProcessorInformation(Some(buf.as_mut_ptr()), &mut len).is_err() {
                return logical;
            }
            let cores = buf.iter().filter(|i| i.Relationship == RelationProcessorCore).count();
            if cores == 0 { logical } else { cores }
        }
    })
}

/// Logical processors (the upper end of the threads stepper).
pub fn logical_cores() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get())
}

/// Free bytes on the volume of `dir` (the nearest existing ancestor).
pub fn free_disk(dir: &Path) -> Option<u64> {
    let mut p = dir;
    while !p.exists() {
        p = p.parent()?;
    }
    let mut free = 0u64;
    unsafe { GetDiskFreeSpaceExW(&HSTRING::from(p.as_os_str()), Some(&mut free), None, None).ok()? };
    Some(free)
}

/// The native folder picker. Blocks; call on its own thread.
pub fn pick_folder(title: &str, start: &Path) -> Option<PathBuf> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree,
    };
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem,
        SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let options = dialog.GetOptions().ok()?;
        dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
        dialog.SetTitle(&HSTRING::from(title)).ok()?;
        let mut p = start;
        while !p.exists() {
            match p.parent() {
                Some(parent) => p = parent,
                None => break,
            }
        }
        if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(p.as_os_str()), None) {
            let _ = dialog.SetFolder(&item);
        }
        dialog.Show(None).ok()?;
        let item = dialog.GetResult().ok()?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        s.map(PathBuf::from)
    }
}

/// A Job object that kills its processes when its last handle closes
/// (that is, when HeliLingo exits or crashes).
pub struct KillOnClose(HANDLE);

// The handle is only used to assign processes and is closed on drop.
unsafe impl Send for KillOnClose {}
unsafe impl Sync for KillOnClose {}

impl KillOnClose {
    pub fn new() -> Option<Self> {
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null()).ok()?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok.is_err() {
                let _ = CloseHandle(job);
                return None;
            }
            Some(Self(job))
        }
    }

    pub fn assign(&self, child: &std::process::Child) -> bool {
        use std::os::windows::io::AsRawHandle;
        unsafe { AssignProcessToJobObject(self.0, HANDLE(child.as_raw_handle())).is_ok() }
    }
}

impl Drop for KillOnClose {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvidia_driver_version() {
        // 32.0.15.9186 → 591.86
        let v = (32u64 << 48) | (15 << 16) | 9186;
        assert_eq!(driver_version(NVIDIA, v), "591.86");
        assert_eq!(driver_version(0x1002, v), "32.0.15.9186");
    }

    #[test]
    fn hardware_queries_dont_panic() {
        assert!(physical_cores() >= 1);
        assert!(logical_cores() >= physical_cores());
        assert!(free_disk(&std::env::temp_dir().join("a/b/c")).is_some());
        let _ = gpu();
        let _ = free_vram();
    }
}
