use fs80h_core::{DeviceInfo, FRAME_LEN, FingerprintScanner, RawFrame};
use fs80h_driver_sys as sys;
use std::{ffi::CStr, ptr::NonNull, sync::Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("FS80H driver status {0:?}: {1}")]
    Status(sys::fs80h_status, String),
    #[error("driver returned a null handle")]
    NullHandle,
    #[error("driver frame geometry is invalid: {0}x{1} @ {2} DPI")]
    Geometry(u32, u32, u16),
    #[error(transparent)]
    Core(#[from] fs80h_core::CoreError),
}
fn message() -> String {
    unsafe {
        let p = sys::fs80h_last_error_message();
        if p.is_null() {
            "no driver message".into()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}
fn check(status: sys::fs80h_status) -> Result<(), DriverError> {
    if status == sys::fs80h_status::FS80H_OK {
        Ok(())
    } else {
        Err(DriverError::Status(status, message()))
    }
}
pub fn abi_version() -> u32 {
    unsafe { sys::fs80h_abi_version() }
}
pub fn list_devices() -> Result<Vec<DeviceInfo>, DriverError> {
    unsafe {
        let mut n = 0;
        check(sys::fs80h_list_devices(std::ptr::null_mut(), 0, &mut n))?;
        let mut items = vec![std::mem::zeroed::<sys::fs80h_device_info>(); n];
        check(sys::fs80h_list_devices(
            items.as_mut_ptr(),
            items.len(),
            &mut n,
        ))?;
        items.truncate(n);
        Ok(items
            .into_iter()
            .map(|d| DeviceInfo {
                vendor_id: d.vendor_id,
                product_id: d.product_id,
                bus_number: d.bus_number,
                address: d.address,
                model: d.model,
            })
            .collect())
    }
}
struct Frame(NonNull<sys::fs80h_frame>);
impl Drop for Frame {
    fn drop(&mut self) {
        unsafe { sys::fs80h_frame_free(self.0.as_ptr()) }
    }
}
struct Buffer(sys::fs80h_buffer);
impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { sys::fs80h_buffer_free(&mut self.0) }
    }
}
pub struct FutronicScanner {
    device: NonNull<sys::fs80h_device>,
    info: Option<DeviceInfo>,
    gate: Mutex<()>,
}
// The C ABI forbids concurrent handle use. Calls are serialized by `gate`; moving
// ownership to another thread is safe because the opaque handle remains uniquely owned.
unsafe impl Send for FutronicScanner {}
impl Drop for FutronicScanner {
    fn drop(&mut self) {
        unsafe { sys::fs80h_device_close(self.device.as_ptr()) }
    }
}
impl FutronicScanner {
    pub fn open_first() -> Result<Self, DriverError> {
        unsafe {
            let mut p = std::ptr::null_mut();
            check(sys::fs80h_device_open_first(&mut p))?;
            Ok(Self {
                device: NonNull::new(p).ok_or(DriverError::NullHandle)?,
                info: None,
                gate: Mutex::new(()),
            })
        }
    }
    pub fn open(info: DeviceInfo) -> Result<Self, DriverError> {
        unsafe {
            let mut p = std::ptr::null_mut();
            check(sys::fs80h_device_open(
                info.bus_number,
                info.address,
                &mut p,
            ))?;
            Ok(Self {
                device: NonNull::new(p).ok_or(DriverError::NullHandle)?,
                info: Some(info),
                gate: Mutex::new(()),
            })
        }
    }
    pub fn device_info(&self) -> Option<DeviceInfo> {
        self.info.clone()
    }
    fn capture_inner(&self, auto: Option<u64>) -> Result<RawFrame, DriverError> {
        let _guard = self.gate.lock().expect("scanner mutex poisoned");
        unsafe {
            let mut p = std::ptr::null_mut();
            check(match auto {
                Some(timeout) => {
                    let options = sys::fs80h_auto_capture_options {
                        timeout_ms: timeout,
                        min_contact_ratio: 0.08,
                        poll_dose: 166,
                        capture_dose: 166,
                        poll_interval_ms: 40,
                        consecutive_stable_frames: 2,
                        stability_tolerance: 0.06,
                        green_led: 1,
                        red_led: 0,
                        wait_for_removal: 0,
                        removal_timeout_ms: 5_000,
                    };
                    let mut report = sys::fs80h_auto_capture_report::default();
                    sys::fs80h_device_auto_capture(
                        self.device.as_ptr(),
                        &options,
                        &mut p,
                        &mut report,
                    )
                }
                None => sys::fs80h_device_capture(self.device.as_ptr(), 0, &mut p),
            })?;
            let f = Frame(NonNull::new(p).ok_or(DriverError::NullHandle)?);
            let (mut w, mut h, mut dpi) = (0, 0, 0);
            check(sys::fs80h_frame_dimensions(
                f.0.as_ptr(),
                &mut w,
                &mut h,
                &mut dpi,
            ))?;
            if w != 320 || h != 480 || dpi != 500 {
                return Err(DriverError::Geometry(w, h, dpi));
            };
            let mut b = Buffer(sys::fs80h_buffer {
                data: std::ptr::null_mut(),
                len: 0,
            });
            check(sys::fs80h_frame_copy_pixels(f.0.as_ptr(), &mut b.0))?;
            if b.0.len != FRAME_LEN || b.0.data.is_null() {
                return Err(DriverError::Geometry(w, h, dpi));
            };
            RawFrame::new(std::slice::from_raw_parts(b.0.data, b.0.len).to_vec())
                .map_err(Into::into)
        }
    }
}
impl FingerprintScanner for FutronicScanner {
    type Error = DriverError;
    fn capture(&mut self) -> Result<RawFrame, Self::Error> {
        self.capture_inner(None)
    }
    fn auto_capture(&mut self, timeout_ms: u64) -> Result<RawFrame, Self::Error> {
        self.capture_inner(Some(timeout_ms))
    }
}
pub fn native_match(
    probe: &RawFrame,
    reference: &RawFrame,
    threshold: f32,
) -> Result<f32, DriverError> {
    unsafe {
        let (mut a, mut b) = (std::ptr::null_mut(), std::ptr::null_mut());
        check(sys::fs80h_frame_from_pixels(
            reference.pixels().as_ptr(),
            FRAME_LEN,
            &mut a,
        ))?;
        check(sys::fs80h_frame_from_pixels(
            probe.pixels().as_ptr(),
            FRAME_LEN,
            &mut b,
        ))?;
        let a = Frame(NonNull::new(a).ok_or(DriverError::NullHandle)?);
        let b = Frame(NonNull::new(b).ok_or(DriverError::NullHandle)?);
        let mut ta = Buffer(sys::fs80h_buffer {
            data: std::ptr::null_mut(),
            len: 0,
        });
        let mut tb = Buffer(sys::fs80h_buffer {
            data: std::ptr::null_mut(),
            len: 0,
        });
        check(sys::fs80h_frame_extract_iso(a.0.as_ptr(), 2011, &mut ta.0))?;
        check(sys::fs80h_frame_extract_iso(b.0.as_ptr(), 2011, &mut tb.0))?;
        let mut m = std::ptr::null_mut();
        check(sys::fs80h_matcher_create(threshold, &mut m))?;
        let m = NonNull::new(m).ok_or(DriverError::NullHandle)?;
        struct Matcher(NonNull<sys::fs80h_matcher>);
        impl Drop for Matcher {
            fn drop(&mut self) {
                unsafe { sys::fs80h_matcher_free(self.0.as_ptr()) }
            }
        }
        let m = Matcher(m);
        let mut result = std::mem::zeroed();
        check(sys::fs80h_matcher_verify_iso(
            m.0.as_ptr(),
            ta.0.data,
            ta.0.len,
            tb.0.data,
            tb.0.len,
            &mut result,
        ))?;
        Ok(result.score)
    }
}

/// Renders a non-destructive operational derivative through the vendored ABI v2.
pub fn export_derived(frame: &RawFrame, derive: &str, format: u32) -> Result<Vec<u8>, DriverError> {
    unsafe {
        let mut source = std::ptr::null_mut();
        check(sys::fs80h_frame_from_pixels(
            frame.pixels().as_ptr(),
            FRAME_LEN,
            &mut source,
        ))?;
        let source = Frame(NonNull::new(source).ok_or(DriverError::NullHandle)?);
        let mut derived = std::ptr::null_mut();
        check(match derive {
            "inverted" => sys::fs80h_frame_inverted(source.0.as_ptr(), &mut derived),
            "normalized" => sys::fs80h_frame_background_normalized(source.0.as_ptr(), &mut derived),
            "enhanced" => sys::fs80h_frame_ridge_enhanced(source.0.as_ptr(), &mut derived),
            _ => {
                return Err(DriverError::Status(
                    sys::fs80h_status::FS80H_INVALID_ARGUMENT,
                    "unknown derivative".into(),
                ));
            }
        })?;
        let derived = Frame(NonNull::new(derived).ok_or(DriverError::NullHandle)?);
        let mut buffer = Buffer(sys::fs80h_buffer {
            data: std::ptr::null_mut(),
            len: 0,
        });
        check(sys::fs80h_frame_encode_image(
            derived.0.as_ptr(),
            format,
            &mut buffer.0,
        ))?;
        if buffer.0.data.is_null() || buffer.0.len == 0 {
            return Err(DriverError::NullHandle);
        }
        Ok(std::slice::from_raw_parts(buffer.0.data, buffer.0.len).to_vec())
    }
}
