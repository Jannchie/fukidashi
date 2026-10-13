use crate::model::ProcessInfo;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// 列出候选进程。
///
/// 游戏一定有可见主窗口，所以先枚举窗口：有窗口的进程按窗口 Z 序（最近激活的在前）
/// 排在最前，并补上窗口标题、文件描述、图标和位数；其余进程排在后面，
/// 系统目录里的再沉底。前端默认只显示有窗口的那一段。
pub fn list() -> Vec<ProcessInfo> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );

    let me = std::process::id();
    let windows = win::app_windows();
    let z = |pid: u32| windows.iter().position(|(p, _)| *p == pid).unwrap_or(usize::MAX);

    let mut out: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .filter(|(pid, _)| pid.as_u32() != me)
        .map(|(pid, p)| {
            let pid = pid.as_u32();
            let path = p.exe().map(|e| e.display().to_string()).unwrap_or_default();
            let title = windows.iter().find(|(p, _)| *p == pid).map(|(_, t)| t.clone());
            let rich = title.is_some() && !path.is_empty();
            ProcessInfo {
                pid,
                name: p.name().to_string_lossy().to_string(),
                description: if rich { cached_description(&path) } else { None },
                icon: if rich { cached_icon(&path) } else { None },
                is64: if rich { exe_is_64bit(&path) } else { None },
                title,
                path,
            }
        })
        .filter(|p| !p.name.is_empty())
        .collect();

    out.sort_by(|a, b| {
        z(a.pid)
            .cmp(&z(b.pid))
            .then_with(|| system_ish(&a.path).cmp(&system_ish(&b.path)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

/// 有可见主窗口的进程：(pid, 进程名, exe 路径, 窗口标题)，按 Z 序。
/// 只查有窗口的那几个进程的路径，比 `list()` 轻得多，适合后台轮询。
pub fn windowed() -> Vec<(u32, String, String, String)> {
    let windows = win::app_windows();
    let pids: Vec<sysinfo::Pid> = windows.iter().map(|(p, _)| sysinfo::Pid::from_u32(*p)).collect();
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    windows
        .into_iter()
        .filter_map(|(pid, title)| {
            let p = sys.process(sysinfo::Pid::from_u32(pid))?;
            let path = p.exe()?.display().to_string();
            Some((pid, p.name().to_string_lossy().into_owned(), path, title))
        })
        .collect()
}

fn system_ish(path: &str) -> u8 {
    let p = path.to_lowercase();
    if p.is_empty() {
        return 2;
    }
    if p.starts_with("c:\\windows") || p.contains("\\system32\\") || p.contains("\\syswow64\\") {
        1
    } else {
        0
    }
}

/// 读 PE 头判断可执行文件的位数。
///
/// 这一步不能省：注入的钩子 dll 与目标进程的位数必须一致，32 位的钩子
/// 进不了 64 位游戏，反之亦然。而多数日文视觉小说到今天仍是 32 位程序。
///
/// 结构：DOS 头开头是 "MZ"，偏移 0x3C 处的 u32 指向 PE 头，
/// PE 头是 "PE\0\0" 之后紧跟 IMAGE_FILE_HEADER，其首字段 Machine 即架构。
pub fn exe_is_64bit(path: &str) -> Option<bool> {
    let mut f = std::fs::File::open(path).ok()?;

    let mut mz = [0u8; 2];
    f.read_exact(&mut mz).ok()?;
    if &mz != b"MZ" {
        return None;
    }

    f.seek(SeekFrom::Start(0x3C)).ok()?;
    let mut lfanew = [0u8; 4];
    f.read_exact(&mut lfanew).ok()?;

    f.seek(SeekFrom::Start(u32::from_le_bytes(lfanew) as u64)).ok()?;
    let mut head = [0u8; 6]; // "PE\0\0" + Machine(u16)
    f.read_exact(&mut head).ok()?;
    if &head[..4] != b"PE\0\0" {
        return None;
    }

    match u16::from_le_bytes([head[4], head[5]]) {
        0x014C => Some(false),          // i386
        0x8664 | 0xAA64 => Some(true),  // amd64 / arm64
        _ => None,
    }
}

/// 描述和图标按 exe 路径缓存：同一个 exe 的这两样不会变，重复打开选择器时直接命中。
static DESCRIPTIONS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);
static ICONS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

fn cached(
    cache: &Mutex<Option<HashMap<String, Option<String>>>>,
    path: &str,
    load: impl FnOnce(&str) -> Option<String>,
) -> Option<String> {
    if let Some(hit) = cache.lock().unwrap().get_or_insert_with(HashMap::new).get(path) {
        return hit.clone();
    }
    let v = load(path);
    cache.lock().unwrap().as_mut().unwrap().insert(path.to_string(), v.clone());
    v
}

fn cached_description(path: &str) -> Option<String> {
    cached(&DESCRIPTIONS, path, win::file_description)
}

fn cached_icon(path: &str) -> Option<String> {
    cached(&ICONS, path, |p| win::icon_rgba(p, 32).and_then(|px| png_data_url(&px, 32)))
}

fn png_data_url(rgba: &[u8], size: u32) -> Option<String> {
    use base64::Engine;
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, size, size);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().ok()?.write_image_data(rgba).ok()?;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(buf)
    ))
}

#[cfg(windows)]
mod win {
    use windows::core::{BOOL, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
    };
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, EnumWindows, GetIconInfo, GetWindow, GetWindowLongPtrW,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        PrivateExtractIconsW, GWL_EXSTYLE, GW_OWNER, HICON, ICONINFO, WS_EX_TOOLWINDOW,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 主窗口：可见、无所有者、非工具窗口、有标题、未被 DWM 隐藏（UWP 后台窗口会被 cloak）。
    /// 返回 (pid, 标题)，按 Z 序从上到下，同一进程只取最上面那个。
    pub fn app_windows() -> Vec<(u32, String)> {
        unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let out = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
            out.push(hwnd);
            BOOL(1)
        }
        let mut all: Vec<HWND> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize));
        }
        let mut out: Vec<(u32, String)> = Vec::new();
        for hwnd in all {
            unsafe {
                if !IsWindowVisible(hwnd).as_bool()
                    || GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid())
                    || GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0
                {
                    continue;
                }
                let mut cloaked = 0u32;
                let _ = DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_CLOAKED,
                    (&mut cloaked as *mut u32).cast(),
                    4,
                );
                if cloaked != 0 {
                    continue;
                }
                let len = GetWindowTextLengthW(hwnd);
                if len == 0 {
                    continue;
                }
                let mut buf = vec![0u16; len as usize + 1];
                let n = GetWindowTextW(hwnd, &mut buf);
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid != 0 && !out.iter().any(|(p, _)| *p == pid) {
                    out.push((pid, String::from_utf16_lossy(&buf[..n.max(0) as usize])));
                }
            }
        }
        out
    }

    /// exe 的文件描述（资源管理器"属性 → 详细信息"里的那个）。
    pub fn file_description(path: &str) -> Option<String> {
        let wpath = wide(path);
        let p = PCWSTR(wpath.as_ptr());
        let size = unsafe { GetFileVersionInfoSizeW(p, None) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        unsafe { GetFileVersionInfoW(p, None, size, data.as_mut_ptr().cast()) }.ok()?;

        let query = |sub: &str| -> Option<(*const u8, usize)> {
            let sub = wide(sub);
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            let ok = unsafe {
                VerQueryValueW(data.as_ptr().cast(), PCWSTR(sub.as_ptr()), &mut ptr, &mut len)
            }
            .as_bool();
            (ok && !ptr.is_null() && len > 0).then_some((ptr as *const u8, len as usize))
        };
        // 先按文件自带的语言/代码页查，再退回常见的英文、中文、日文。
        let mut langs: Vec<String> = Vec::new();
        if let Some((ptr, len)) = query(r"\VarFileInfo\Translation") {
            for i in 0..len / 4 {
                let pair = unsafe { std::slice::from_raw_parts(ptr.add(i * 4) as *const u16, 2) };
                langs.push(format!("{:04x}{:04x}", pair[0], pair[1]));
            }
        }
        langs.extend(["040904b0", "040904e4", "041104b0", "080404b0", "000004b0"].map(String::from));
        langs.iter().find_map(|lang| {
            let (ptr, len) = query(&format!(r"\StringFileInfo\{lang}\FileDescription"))?;
            let units = unsafe { std::slice::from_raw_parts(ptr as *const u16, len) };
            let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
            let s = String::from_utf16_lossy(&units[..end]).trim().to_string();
            (!s.is_empty()).then_some(s)
        })
    }

    /// 从 exe 里取指定尺寸的图标，返回 RGBA 像素。
    pub fn icon_rgba(path: &str, size: i32) -> Option<Vec<u8>> {
        let mut name = [0u16; 260];
        let units: Vec<u16> = path.encode_utf16().collect();
        if units.len() >= name.len() {
            return None;
        }
        name[..units.len()].copy_from_slice(&units);

        let mut icons = [HICON::default()];
        let n = unsafe { PrivateExtractIconsW(&name, 0, size, size, Some(&mut icons), None, 0) };
        if n == 0 || n == u32::MAX || icons[0].is_invalid() {
            return None;
        }
        let px = to_rgba(icons[0], size);
        unsafe {
            let _ = DestroyIcon(icons[0]);
        }
        px
    }

    fn to_rgba(icon: HICON, size: i32) -> Option<Vec<u8>> {
        let mut info = ICONINFO::default();
        unsafe { GetIconInfo(icon, &mut info) }.ok()?;
        let dc = unsafe { CreateCompatibleDC(None) };
        let result = read_bits(dc, info.hbmColor, size).map(|mut px| {
            // 老式图标没有 alpha 通道，用 AND 掩码补：掩码为 0 的像素不透明。
            if px.chunks_exact(4).all(|p| p[3] == 0) {
                let mask = read_bits(dc, info.hbmMask, size);
                for (i, p) in px.chunks_exact_mut(4).enumerate() {
                    p[3] = match &mask {
                        Some(m) if m[i * 4] != 0 => 0,
                        _ => 255,
                    };
                }
            }
            for p in px.chunks_exact_mut(4) {
                p.swap(0, 2); // BGRA → RGBA
            }
            px
        });
        unsafe {
            let _ = DeleteDC(dc);
            if !info.hbmColor.is_invalid() {
                let _ = DeleteObject(info.hbmColor.into());
            }
            if !info.hbmMask.is_invalid() {
                let _ = DeleteObject(info.hbmMask.into());
            }
        }
        result
    }

    fn read_bits(dc: HDC, bitmap: HBITMAP, size: i32) -> Option<Vec<u8>> {
        if bitmap.is_invalid() {
            return None;
        }
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size, // 自上而下
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (size * size * 4) as usize];
        let lines = unsafe {
            GetDIBits(
                dc,
                bitmap,
                0,
                size as u32,
                Some(px.as_mut_ptr().cast()),
                &mut bmi,
                DIB_RGB_COLORS,
            )
        };
        (lines == size).then_some(px)
    }
}

#[cfg(not(windows))]
mod win {
    pub fn app_windows() -> Vec<(u32, String)> {
        Vec::new()
    }
    pub fn file_description(_: &str) -> Option<String> {
        None
    }
    pub fn icon_rgba(_: &str, _: i32) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::exe_is_64bit;

    /// 拿系统自带的程序当样本，避免依赖仓库里的二进制。
    #[test]
    fn detects_system_binaries() {
        // 64 位系统上 System32 下的是 64 位，SysWOW64 下的是 32 位
        if let Some(b) = exe_is_64bit(r"C:\Windows\System32\notepad.exe") {
            assert!(b, "System32 下应为 64 位");
        }
        if let Some(b) = exe_is_64bit(r"C:\Windows\SysWOW64\notepad.exe") {
            assert!(!b, "SysWOW64 下应为 32 位");
        }
    }

    #[test]
    fn rejects_non_pe() {
        assert_eq!(exe_is_64bit("Cargo.toml"), None);
        assert_eq!(exe_is_64bit("definitely/not/here.exe"), None);
    }
}
