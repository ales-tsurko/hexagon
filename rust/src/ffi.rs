#![allow(unreachable_pub)]

use std::ffi::{c_char, CStr};

use ustr::Ustr;

#[no_mangle]
pub extern "C" fn ustr(chars: *const c_char) -> Ustr {
    let cs = unsafe { CStr::from_ptr(chars).to_string_lossy() };
    Ustr::from(&cs)
}

#[no_mangle]
pub extern "C" fn listen(event: Ustr, callback: extern "C" fn(*const u8)) {
    // ...
}

#[no_mangle]
pub extern "C" fn unlisten(event: Ustr) {
    // ...
}

#[no_mangle]
pub extern "C" fn send(event: Ustr, data: &[u8]) {
    // ...
}