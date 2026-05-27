use std::ffi::CString;
use std::os::raw::{c_int, c_char};

#[link(name = "log")]
extern "C" {
    pub fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

pub fn is_debug_enabled() -> bool {
    let name = CString::new("persist.sys.richtap.debug").unwrap();
    let mut value = vec![0u8; libc::PROP_VALUE_MAX as usize];
    unsafe {
        let len = libc::__system_property_get(name.as_ptr(), value.as_mut_ptr() as *mut libc::c_char);
        if len > 0 {
            let val = String::from_utf8_lossy(&value[..len as usize]);
            val == "1" || val == "true"
        } else {
            false
        }
    }
}

#[macro_export]
macro_rules! hal_log {
    ($($arg:tt)*) => {
        if $crate::logger::is_debug_enabled() {
            let formatted = format!($($arg)*);
            let safe_string = formatted.replace('\0', "");
            
            if let Ok(msg) = std::ffi::CString::new(safe_string) {
                if let Ok(tag) = std::ffi::CString::new("RichtapHAL") {
                    unsafe {
                        $crate::logger::__android_log_write(
                            4, 
                            tag.as_ptr() as *const std::os::raw::c_char,
                            msg.as_ptr() as *const std::os::raw::c_char,
                        );
                    }
                }
            }
        }
    }
}