#![allow(non_camel_case_types)]
use std::ffi::c_char;

#[repr(C)]
pub struct fs80h_device {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fs80h_frame {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fs80h_matcher {
    _private: [u8; 0],
}
#[repr(C)]
pub struct fs80h_auto_capture_options {
    pub timeout_ms: u64,
    pub min_contact_ratio: f32,
    pub poll_dose: u8,
    pub capture_dose: u8,
    pub poll_interval_ms: u32,
    pub consecutive_stable_frames: u32,
    pub stability_tolerance: f32,
    pub green_led: u8,
    pub red_led: u8,
    pub wait_for_removal: u8,
    pub removal_timeout_ms: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct fs80h_presence_metrics {
    pub contact_ratio: f32,
    pub active_blocks: u32,
    pub total_blocks: u32,
    pub is_present: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct fs80h_auto_capture_report {
    pub final_contact: fs80h_presence_metrics,
    pub removal_waited: u8,
}
#[repr(C)]
pub struct fs80h_buffer {
    pub data: *mut u8,
    pub len: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct fs80h_device_info {
    pub vendor_id: u16,
    pub product_id: u16,
    pub bus_number: u8,
    pub address: u8,
    pub model: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct fs80h_match_result {
    pub score: f32,
    pub is_match: u8,
    pub matched_minutiae: usize,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum fs80h_status {
    FS80H_OK = 0,
    FS80H_INVALID_ARGUMENT = 1,
    FS80H_BUFFER_TOO_SMALL = 2,
    FS80H_NOT_FOUND = 3,
    FS80H_TIMEOUT = 4,
    FS80H_UNSUPPORTED = 5,
    FS80H_ERROR = 255,
}

unsafe extern "C" {
    pub fn fs80h_abi_version() -> u32;
    pub fn fs80h_version() -> *const c_char;
    pub fn fs80h_last_error_message() -> *const c_char;
    pub fn fs80h_list_devices(
        devices: *mut fs80h_device_info,
        capacity: usize,
        written: *mut usize,
    ) -> fs80h_status;
    pub fn fs80h_device_open_first(out: *mut *mut fs80h_device) -> fs80h_status;
    pub fn fs80h_device_open(bus: u8, address: u8, out: *mut *mut fs80h_device) -> fs80h_status;
    pub fn fs80h_device_close(device: *mut fs80h_device);
    pub fn fs80h_device_capture(
        device: *mut fs80h_device,
        dose: u8,
        out: *mut *mut fs80h_frame,
    ) -> fs80h_status;
    pub fn fs80h_device_auto_capture(
        device: *mut fs80h_device,
        options: *const fs80h_auto_capture_options,
        out: *mut *mut fs80h_frame,
        report: *mut fs80h_auto_capture_report,
    ) -> fs80h_status;
    pub fn fs80h_frame_free(frame: *mut fs80h_frame);
    pub fn fs80h_frame_dimensions(
        frame: *const fs80h_frame,
        width: *mut u32,
        height: *mut u32,
        dpi: *mut u16,
    ) -> fs80h_status;
    pub fn fs80h_frame_copy_pixels(
        frame: *const fs80h_frame,
        out: *mut fs80h_buffer,
    ) -> fs80h_status;
    pub fn fs80h_frame_inverted(
        frame: *const fs80h_frame,
        out: *mut *mut fs80h_frame,
    ) -> fs80h_status;
    pub fn fs80h_frame_background_normalized(
        frame: *const fs80h_frame,
        out: *mut *mut fs80h_frame,
    ) -> fs80h_status;
    pub fn fs80h_frame_ridge_enhanced(
        frame: *const fs80h_frame,
        out: *mut *mut fs80h_frame,
    ) -> fs80h_status;
    pub fn fs80h_frame_encode_image(
        frame: *const fs80h_frame,
        format: u32,
        out: *mut fs80h_buffer,
    ) -> fs80h_status;
    pub fn fs80h_buffer_free(buffer: *mut fs80h_buffer);
    pub fn fs80h_frame_from_pixels(
        data: *const u8,
        len: usize,
        out: *mut *mut fs80h_frame,
    ) -> fs80h_status;
    pub fn fs80h_frame_extract_iso(
        frame: *const fs80h_frame,
        format: u32,
        out: *mut fs80h_buffer,
    ) -> fs80h_status;
    pub fn fs80h_matcher_create(threshold: f32, out: *mut *mut fs80h_matcher) -> fs80h_status;
    pub fn fs80h_matcher_free(matcher: *mut fs80h_matcher);
    pub fn fs80h_matcher_verify_iso(
        matcher: *const fs80h_matcher,
        enrolled: *const u8,
        enrolled_len: usize,
        probe: *const u8,
        probe_len: usize,
        out: *mut fs80h_match_result,
    ) -> fs80h_status;
}
