//! This crate provides callback-based interoperability between Rust and C ABI-compatible language.
#![warn(
    clippy::all,
    deprecated_in_future,
    missing_docs,
    unused_import_braces,
    unused_labels,
    unused_lifetimes,
    unused_qualifications,
    unreachable_pub
)]

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rosc::address::{self, Matcher, OscAddress};
use rosc::{OscError, OscMessage, OscPacket, OscType};
use ustr::{Ustr, UstrMap};

pub(crate) static CALLBACK_STORAGE: Lazy<RwLock<UstrMap<Callback>>> =
    Lazy::new(|| RwLock::new(UstrMap::default()));

#[derive(Debug, Clone, Copy)]
struct Callback(fn(OscType));

impl Callback {
    fn call(&self, data: OscType) {
        (self.0)(data);
    }
}

/// Listen for messages on address.
pub fn listen(address: Ustr, callback: fn(OscType)) -> Result<(), OscError> {
    address::verify_address(&address)?;
    let mut storage = CALLBACK_STORAGE.write();
    (*storage).insert(address, Callback(callback));

    Ok(())
}

/// Send message.
pub fn send(message: OscMessage) -> Result<(), OscError> {
    let storage = ffi::CALLBACK_STORAGE.read();
    let matcher = Matcher::new(&message.addr)?;
    let packet = OscPacket::Message(message);
    let bytes = rosc::encoder::encode(&packet)?;

    storage
        .iter()
        .filter_map(|(key, val)| {
            let address = OscAddress::new(key.to_string()).unwrap();
            if matcher.match_address(&address) {
                Some(val)
            } else {
                None
            }
        })
        .for_each(|callback| callback.call(&bytes));

    Ok(())
}

/// Stop listening for events on address.
pub fn unlisten(address: Ustr) {
    let mut storage = CALLBACK_STORAGE.write();
    (*storage).remove(&address);
}

mod ffi {
    #![allow(unreachable_pub)]

    use std::ffi::{c_char, CStr};

    use once_cell::sync::Lazy;
    use parking_lot::RwLock;
    use rosc::address::{self, Matcher, OscAddress};
    use rosc::OscPacket;
    use ustr::{Ustr, UstrMap};

    pub(crate) static CALLBACK_STORAGE: Lazy<RwLock<UstrMap<Callback>>> =
        Lazy::new(|| RwLock::new(UstrMap::default()));

    #[derive(Debug, Clone, Copy)]
    pub(crate) struct Callback(extern "C" fn(*const u8));

    impl Callback {
        pub(crate) fn call(&self, data: &[u8]) {
            (self.0)(data.as_ptr());
        }
    }

    #[no_mangle]
    pub extern "C" fn ustr(chars: *const c_char) -> Ustr {
        let cs = unsafe { CStr::from_ptr(chars).to_string_lossy() };
        Ustr::from(&cs)
    }

    #[no_mangle]
    pub extern "C" fn listen(address: Ustr, callback: extern "C" fn(*const u8)) {
        address::verify_address(&address).expect("Invalid address");
        let mut storage = CALLBACK_STORAGE.write();
        (*storage).insert(address, Callback(callback));
    }

    #[no_mangle]
    pub extern "C" fn unlisten(address: Ustr) {
        let mut storage = CALLBACK_STORAGE.write();
        (*storage).remove(&address);
    }

    #[no_mangle]
    pub extern "C" fn send(address: Ustr, data: *const u8, data_size: usize) {
        let storage = super::CALLBACK_STORAGE.read();
        let matcher = Matcher::new(&address).expect("Invalid address pattern");
        let data = unsafe { std::slice::from_raw_parts(data, data_size) };
        let packet = rosc::decoder::decode_udp(data).expect("Error decoding OSC packet");
        let data = match packet {
            (_, OscPacket::Message(mut msg)) => msg.args.remove(0),
            _ => panic!("Bundles are not supported"),
        };

        storage
            .iter()
            .filter_map(|(key, val)| {
                let address = OscAddress::new(key.to_string()).unwrap();
                if matcher.match_address(&address) {
                    Some(val)
                } else {
                    None
                }
            })
            .for_each(|callback| {
                callback.call(data.clone());
            });
    }
}
