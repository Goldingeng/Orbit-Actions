use windows::core::{Interface, HSTRING, PCWSTR};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};
use windows::Win32::Storage::EnhancedStorage::{PKEY_AppUserModel_ID, PKEY_ItemNameDisplay};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, IShellItem2, IShellItemImageFactory, SHCreateItemFromParsingName,
    SHGetKnownFolderItem, KF_FLAG_DEFAULT, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};

#[derive(Clone, Debug)]
pub struct App {
    pub name: String,
    pub id: String,
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

fn take(p: windows::core::PWSTR) -> String {
    unsafe {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as _));
        s
    }
}

pub fn list() -> Vec<App> {
    init();
    let mut out = vec![];
    unsafe {
        let Ok(folder) = SHGetKnownFolderItem::<_, IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) else { return out };
        let Ok(items) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else { return out };
        loop {
            let mut batch = [None];
            let mut fetched = 0;
            if items.Next(&mut batch, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = batch[0].take() else { break };
            let Ok(item2) = item.cast::<IShellItem2>() else { continue };
            let (Ok(name), Ok(id)) = (item2.GetString(&PKEY_ItemNameDisplay), item2.GetString(&PKEY_AppUserModel_ID)) else { continue };
            out.push(App { name: take(name), id: take(id) });
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

pub fn icon(id: &str, size: i32) -> Option<Image> {
    init();
    unsafe {
        let path = HSTRING::from(format!("shell:AppsFolder\\{id}"));
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None).ok()?;
        let factory: IShellItemImageFactory = item.cast().ok()?;
        let bitmap = factory.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        let image = read(bitmap);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        image
    }
}

unsafe fn read(bitmap: HBITMAP) -> Option<Image> {
    let mut info = BITMAP::default();
    if GetObjectW(HGDIOBJ(bitmap.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut info as *mut BITMAP as _)) == 0 {
        return None;
    }
    let (w, h) = (info.bmWidth, info.bmHeight.abs());
    let mut header = BITMAPINFO::default();
    header.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut bgra = vec![0u8; (w * h * 4) as usize];
    let dc = GetDC(None);
    let lines = GetDIBits(dc, bitmap, 0, h as u32, Some(bgra.as_mut_ptr() as _), &mut header, DIB_RGB_COLORS);
    ReleaseDC(None, dc);
    if lines == 0 {
        return None;
    }
    let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
    let mut rgba = Vec::with_capacity(bgra.len());
    for p in bgra.chunks_exact(4) {
        let a = if has_alpha { p[3] } else { 255 };
        rgba.extend_from_slice(&[p[2], p[1], p[0], a]);
    }
    Some(Image { width: w as u32, height: h as u32, rgba })
}
