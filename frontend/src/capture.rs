//! Capture this renderer's native window when its platform has no GPU readback implementation.

use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub fn save(window: &Window, path: &str) -> Result<(), String> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| error.to_string())?
        .as_raw();
    #[cfg(target_os = "linux")]
    match handle {
        RawWindowHandle::Xlib(handle) => return x11::save(handle.window, path),
        RawWindowHandle::Xcb(handle) => return x11::save(u64::from(handle.window.get()), path),
        _ => {}
    }
    #[cfg(windows)]
    if let RawWindowHandle::Win32(handle) = handle {
        return windows::save(handle.hwnd.get() as *mut std::ffi::c_void, path);
    }
    Err("native window capture is unavailable on this display".into())
}

#[cfg(target_os = "linux")]
mod x11 {
    use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};
    #[repr(C)]
    struct XImage {
        width: c_int,
        height: c_int,
        xoffset: c_int,
        format: c_int,
        data: *mut c_char,
        byte_order: c_int,
        bitmap_unit: c_int,
        bitmap_bit_order: c_int,
        bitmap_pad: c_int,
        depth: c_int,
        bytes_per_line: c_int,
        bits_per_pixel: c_int,
        red_mask: c_ulong,
        green_mask: c_ulong,
        blue_mask: c_ulong,
    }
    #[link(name = "X11")]
    unsafe extern "C" {
        fn XOpenDisplay(name: *const c_char) -> *mut c_void;
        fn XCloseDisplay(display: *mut c_void) -> c_int;
        fn XGetGeometry(
            display: *mut c_void,
            drawable: c_ulong,
            root: *mut c_ulong,
            x: *mut c_int,
            y: *mut c_int,
            width: *mut c_uint,
            height: *mut c_uint,
            border: *mut c_uint,
            depth: *mut c_uint,
        ) -> c_int;
        fn XGetImage(
            display: *mut c_void,
            drawable: c_ulong,
            x: c_int,
            y: c_int,
            width: c_uint,
            height: c_uint,
            planes: c_ulong,
            format: c_int,
        ) -> *mut XImage;
        fn XGetPixel(image: *mut XImage, x: c_int, y: c_int) -> c_ulong;
        fn XDestroyImage(image: *mut XImage) -> c_int;
    }
    struct Display(*mut c_void);
    impl Drop for Display {
        fn drop(&mut self) {
            unsafe {
                XCloseDisplay(self.0);
            }
        }
    }
    struct Image(*mut XImage);
    impl Drop for Image {
        fn drop(&mut self) {
            unsafe {
                XDestroyImage(self.0);
            }
        }
    }
    fn channel(pixel: c_ulong, mask: c_ulong) -> u8 {
        if mask == 0 {
            return 0;
        }
        let shift = mask.trailing_zeros();
        (((pixel & mask) >> shift) * 255 / (mask >> shift)) as u8
    }
    pub fn save(window: c_ulong, path: &str) -> Result<(), String> {
        // SAFETY: Xlib owns the display and image; RAII closes both on every return. The drawable
        // comes from this GPUI Window, never the root window or a desktop coordinate.
        unsafe {
            let display = Display(XOpenDisplay(std::ptr::null()));
            if display.0.is_null() {
                std::mem::forget(display);
                return Err("can't open the window's X11 display".into());
            }
            let (mut root, mut x, mut y, mut width, mut height, mut border, mut depth) = (0, 0, 0, 0, 0, 0, 0);
            if XGetGeometry(
                display.0,
                window,
                &mut root,
                &mut x,
                &mut y,
                &mut width,
                &mut height,
                &mut border,
                &mut depth,
            ) == 0
            {
                return Err("can't read the window size".into());
            }
            let image = Image(XGetImage(display.0, window, 0, 0, width, height, c_ulong::MAX, 2));
            if image.0.is_null() {
                std::mem::forget(image);
                return Err("can't capture the window".into());
            }
            let masks = ((*image.0).red_mask, (*image.0).green_mask, (*image.0).blue_mask);
            let mut output = image::RgbaImage::new(width, height);
            for (x, y, pixel) in output.enumerate_pixels_mut() {
                let value = XGetPixel(image.0, x as c_int, y as c_int);
                *pixel = image::Rgba([
                    channel(value, masks.0),
                    channel(value, masks.1),
                    channel(value, masks.2),
                    255,
                ]);
            }
            output.save(path).map_err(|error| error.to_string())
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    use windows_sys::Win32::{Foundation::RECT, Graphics::Gdi::*, UI::WindowsAndMessaging::*};
    pub fn save(window: *mut c_void, path: &str) -> Result<(), String> {
        // SAFETY: capture only this window's client DC. Every GDI object and DC is released below.
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            if GetClientRect(window, &mut rect) == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
            let source = GetDC(window);
            if source.is_null() {
                return Err("can't read the window DC".into());
            }
            let target = CreateCompatibleDC(source);
            let bitmap = CreateCompatibleBitmap(source, width, height);
            if target.is_null() || bitmap.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                if !target.is_null() {
                    DeleteDC(target);
                }
                ReleaseDC(window, source);
                return Err("can't allocate the window capture".into());
            }
            let old = SelectObject(target, bitmap);
            let captured = BitBlt(target, 0, 0, width, height, source, 0, 0, SRCCOPY);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = width;
            info.bmiHeader.biHeight = -height;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            let mut bytes = vec![0; width as usize * height as usize * 4];
            SelectObject(target, old);
            let copied = GetDIBits(
                target,
                bitmap,
                0,
                height as u32,
                bytes.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            );
            DeleteObject(bitmap);
            DeleteDC(target);
            ReleaseDC(window, source);
            if captured == 0 || copied != height {
                return Err("can't capture the window pixels".into());
            }
            for pixel in bytes.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                pixel[3] = 255;
            }
            image::RgbaImage::from_raw(width as u32, height as u32, bytes)
                .ok_or("invalid window pixels")?
                .save(path)
                .map_err(|error| error.to_string())
        }
    }
}
