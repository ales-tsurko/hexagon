//! This crate provides interoperability between Rust and Haxe programming language.
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

mod ffi;

use std::thread::{self, JoinHandle};

use bytes::Bytes;
use kanal::{Receiver, Sender};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::runtime::{Handle, Runtime};
use ustr::{Ustr, UstrMap};

pub(crate) static CALLBACK_STORAGE: Lazy<RwLock<UstrMap<Callback>>> =
    Lazy::new(|| RwLock::new(UstrMap::default()));
static TOKIO_RUNTIME: Lazy<Runtime> =
    Lazy::new(|| Runtime::new().expect("Error creating tokio runtime"));
pub(crate) static CHANNEL: Lazy<(Sender<Message>, Receiver<Message>)> =
    Lazy::new(|| kanal::unbounded());

pub(crate) struct Callback(extern "C" fn(*const u8));

impl Callback {
    pub(crate) fn call(&self, data: &[u8]) {
        (self.0)(data.as_ptr());
    }
}

/// Implement this trait for the type which will handle events.
pub trait Dispatcher: Send + Sync + 'static {
    /// Called when an event is received.
    fn receive<'de, Data: Deserialize<'de>>(&self, event: Ustr, data: Data);

    /// Send an event to the Haxe side.
    fn send<Data: Serialize>(&self, event: Ustr, data: Data) -> Result<(), Error> {
        let storage = CALLBACK_STORAGE.read();

        match storage.get(&event) {
            Some(callback) => {
                let bytes = bincode::serialize(&data)?;
                callback.call(&bytes);
                Ok(())
            }
            None => Err(Error::CallbackNotFound(event)),
        }
    }
}

/// Start the event loop in the async runtime.
///
/// The function returns a [`Handle`], which can be used to spawn tasks.
pub fn start_async<D: Dispatcher>(dispatcher: D) -> Result<Handle, Error> {
    let handle = async_runtime_handle();

    handle.spawn(async move {
        let channel = CHANNEL.1.clone_async();
        while !CHANNEL.1.is_closed() {
            match channel.recv().await {
                Ok(message) => dispatcher.receive(message.event, message.data),
                Err(err) => log::error!("Error receiving message: {}", err),
            }
        }
    });

    Ok(handle)
}

/// Start a new thread and run the event loop in it.
///
/// The function returns a [`JoinHandle`], although if you try to join it, it will block your
/// thread, because of the loop.
pub fn start_thread<D: Dispatcher>(dispatcher: D) -> Result<JoinHandle<()>, Error> {
    let channel = CHANNEL.1.clone();

    Ok(thread::spawn(move || {
        while !channel.is_closed() {
            match channel.recv() {
                Ok(message) => dispatcher.receive(message.event, message.data),
                Err(err) => log::error!("Error receiving message: {}", err),
            }
        }
    }))
}

/// Stop all event loops.
pub fn stop() {
    CHANNEL.0.close();
}

pub(crate) fn async_runtime_handle() -> Handle {
    TOKIO_RUNTIME.handle().clone()
}

#[derive(Debug)]
struct Message {
    event: Ustr,
    data: Bytes,
}

/// Error type.
#[derive(Error, Debug)]
pub enum Error {
    /// Error initializing callback storage.
    #[error("Callback for event '{0}' is not registered.")]
    CallbackNotFound(Ustr),
    /// Error serializing data.
    #[error("Error serializing data: {0}")]
    SerializingData(#[from] bincode::Error),
}
