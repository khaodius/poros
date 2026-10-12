//! The operating system's own file and folder icons, for file lists that show them instead of
//! Poros's: the shell's icons on Windows, and the desktop's icon theme on Linux. Icons depend
//! only on the name and kind asked about, so remote files get them as local ones do.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Deserialize;
use tauri::AppHandle;

use crate::error::{AppError, AppResult, ErrorKind};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconRequest {
    /// A file name: its extension decides the icon, or on Linux its whole name when it has none.
    pub name: String,
    pub folder: bool,
    /// A symbolic link, drawn with the system's link overlay.
    pub link: bool,
}

/// Straight (not premultiplied) RGBA pixels, row after row.
struct Pixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Each icon as a PNG data URL, or none where the system has no icon for it. The shell and GTK
/// both want the main thread, so the icons are looked up there.
pub async fn icons(
    app: &AppHandle,
    requests: Vec<IconRequest>,
    size: u32,
) -> AppResult<Vec<Option<String>>> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let icons = requests
            .iter()
            .map(|request| platform::icon(request, size).and_then(|pixels| data_url(&pixels)))
            .collect::<Vec<_>>();
        let _ = sender.send(icons);
    })
    .map_err(|error| AppError::new(ErrorKind::Io, format!("Could not look up icons: {error}")))?;
    receiver
        .await
        .map_err(|_| AppError::new(ErrorKind::Io, "Looking up icons stopped before it finished"))
}

fn data_url(pixels: &Pixels) -> Option<String> {
    let expected = (pixels.width as usize) * (pixels.height as usize) * 4;
    if pixels.width == 0 || pixels.height == 0 || pixels.rgba.len() != expected {
        return None;
    }
    let mut encoded = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded, pixels.width, pixels.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&pixels.rgba).ok()?;
    writer.finish().ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(encoded)
    ))
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;

    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    };
    use windows_sys::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_LINKOVERLAY,
        SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DestroyIcon, GetIconInfo, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON,
    };

    use super::{IconRequest, Pixels};

    /// The icon Explorer shows for files of this name, at the system's small icon size, or the
    /// large one when that is nearer `size`. The file need not exist.
    pub fn icon(request: &IconRequest, size: u32) -> Option<Pixels> {
        let small = u32::try_from(unsafe { GetSystemMetrics(SM_CXSMICON) }).unwrap_or(16);
        let scale = if size > small + small / 2 {
            SHGFI_LARGEICON
        } else {
            SHGFI_SMALLICON
        };
        let overlay = if request.link { SHGFI_LINKOVERLAY } else { 0 };
        let attributes = if request.folder {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_NORMAL
        };
        let name: Vec<u16> = request.name.encode_utf16().chain([0]).collect();
        let mut info: SHFILEINFOW = unsafe { zeroed() };
        let found = unsafe {
            SHGetFileInfoW(
                name.as_ptr(),
                attributes,
                &mut info,
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_USEFILEATTRIBUTES | scale | overlay,
            )
        };
        if found == 0 || info.hIcon.is_null() {
            return None;
        }
        let pixels = unsafe { icon_pixels(info.hIcon) };
        unsafe { DestroyIcon(info.hIcon) };
        pixels
    }

    unsafe fn icon_pixels(icon: HICON) -> Option<Pixels> {
        let mut info: ICONINFO = zeroed();
        if GetIconInfo(icon, &mut info) == 0 {
            return None;
        }
        let pixels = color_icon_pixels(&info);
        if !info.hbmColor.is_null() {
            DeleteObject(info.hbmColor);
        }
        if !info.hbmMask.is_null() {
            DeleteObject(info.hbmMask);
        }
        pixels
    }

    /// Icons from before alpha channels have none, and take their transparency from the mask.
    unsafe fn color_icon_pixels(info: &ICONINFO) -> Option<Pixels> {
        if info.hbmColor.is_null() {
            return None;
        }
        let mut bitmap: BITMAP = zeroed();
        let read = GetObjectW(
            info.hbmColor,
            size_of::<BITMAP>() as i32,
            (&mut bitmap as *mut BITMAP).cast::<c_void>(),
        );
        if read == 0 || bitmap.bmWidth <= 0 || bitmap.bmHeight <= 0 {
            return None;
        }
        let (width, height) = (bitmap.bmWidth, bitmap.bmHeight);
        let color = bitmap_bgra(info.hbmColor, width, height)?;
        let (color_pixels, _) = color.as_chunks::<4>();
        let has_alpha = color_pixels.iter().any(|pixel| pixel[3] != 0);
        let mask = if has_alpha {
            None
        } else {
            bitmap_bgra(info.hbmMask, width, height)
        };
        let mut rgba = Vec::with_capacity(color.len());
        for (index, pixel) in color_pixels.iter().enumerate() {
            let alpha = if has_alpha {
                pixel[3]
            } else if mask.as_ref().is_some_and(|mask| mask[index * 4] != 0) {
                0
            } else {
                255
            };
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], alpha]);
        }
        Some(Pixels {
            width: width as u32,
            height: height as u32,
            rgba,
        })
    }

    /// A bitmap's pixels as 32-bit BGRA, top row first.
    unsafe fn bitmap_bgra(bitmap: HBITMAP, width: i32, height: i32) -> Option<Vec<u8>> {
        if bitmap.is_null() {
            return None;
        }
        let context = CreateCompatibleDC(null_mut());
        if context.is_null() {
            return None;
        }
        let mut header: BITMAPINFO = zeroed();
        header.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative for rows from the top down.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        };
        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        let lines = GetDIBits(
            context,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr().cast::<c_void>(),
            &mut header,
            DIB_RGB_COLORS,
        );
        DeleteDC(context);
        (lines == height).then_some(pixels)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use gtk::gio;
    use gtk::prelude::*;
    use gtk::{IconLookupFlags, IconTheme};

    use super::{IconRequest, Pixels};

    /// The icon the desktop's icon theme has for the file's type, guessed from its name as file
    /// managers do, with the theme's link emblem on links.
    pub fn icon(request: &IconRequest, size: u32) -> Option<Pixels> {
        let theme = IconTheme::default()?;
        let content_type = if request.folder {
            "inode/directory".into()
        } else {
            gio::content_type_guess(Some(&request.name), &[]).0
        };
        let base = gio::content_type_get_icon(&content_type);
        let icon: gio::Icon = if request.link {
            let emblem = gio::Emblem::new(&gio::ThemedIcon::new("emblem-symbolic-link"));
            gio::EmblemedIcon::new(&base, Some(&emblem)).upcast()
        } else {
            base
        };
        let size = i32::try_from(size).ok()?;
        let pixbuf = theme
            .lookup_by_gicon(&icon, size, IconLookupFlags::FORCE_SIZE)?
            .load_icon()
            .ok()?;
        if pixbuf.bits_per_sample() != 8 {
            return None;
        }
        let width = usize::try_from(pixbuf.width()).ok()?;
        let height = usize::try_from(pixbuf.height()).ok()?;
        let stride = usize::try_from(pixbuf.rowstride()).ok()?;
        let channels = usize::try_from(pixbuf.n_channels()).ok()?;
        let bytes = pixbuf.read_pixel_bytes();
        let mut rgba = Vec::with_capacity(width * height * 4);
        for row in 0..height {
            for column in 0..width {
                let start = row * stride + column * channels;
                let pixel = bytes.get(start..start + channels)?;
                rgba.extend_from_slice(&pixel[..3]);
                rgba.push(if channels == 4 { pixel[3] } else { 255 });
            }
        }
        Some(Pixels {
            width: width as u32,
            height: height as u32,
            rgba,
        })
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::{IconRequest, Pixels};

    pub fn icon(_request: &IconRequest, _size: u32) -> Option<Pixels> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_pixels_as_a_png_data_url() {
        let pixels = Pixels {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 128],
        };
        let url = data_url(&pixels).unwrap();
        let encoded = url.strip_prefix("data:image/png;base64,").unwrap();
        let bytes = STANDARD.decode(encoded).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        let frame = reader.next_frame(&mut decoded).unwrap();
        assert_eq!((frame.width, frame.height), (2, 1));
        assert_eq!(&decoded[..frame.buffer_size()], &pixels.rgba[..]);
    }

    #[test]
    fn refuses_pixels_that_do_not_fill_the_size() {
        let pixels = Pixels {
            width: 2,
            height: 2,
            rgba: vec![0; 12],
        };
        assert!(data_url(&pixels).is_none());
    }
}
