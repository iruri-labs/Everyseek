use crate::Engine;
use serde_json::json;
use std::{
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

fn output(value: serde_json::Value) -> *mut c_char {
    CString::new(value.to_string())
        .expect("JSON escapes NUL")
        .into_raw()
}

/// Returned engine is owned by the caller. Error strings use em_string_free.
/// All C strings must be valid NUL-terminated UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn em_open(path: *const c_char, error: *mut *mut c_char) -> *mut Engine {
    if !error.is_null() {
        unsafe {
            *error = ptr::null_mut();
        }
    }
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<Engine, String> {
        if path.is_null() {
            return Err("Missing database path".into());
        }
        let path = unsafe { CStr::from_ptr(path) }
            .to_str()
            .map_err(|e| e.to_string())?;
        Engine::open(path).map_err(|e| e.to_string())
    }))
    .unwrap_or_else(|_| Err("Index initialization panicked".into()));
    match result {
        Ok(engine) => Box::into_raw(Box::new(engine)),
        Err(message) => {
            if !error.is_null() {
                unsafe {
                    *error = output(json!({"error":message}));
                }
            }
            ptr::null_mut()
        }
    }
}
/// The handle must remain alive until all requests and cancellations return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn em_request(engine: *const Engine, request: *const c_char) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<serde_json::Value, String> {
        if engine.is_null() || request.is_null() {
            return Err("Invalid core request".into());
        }
        let request = unsafe { CStr::from_ptr(request) }
            .to_str()
            .map_err(|e| e.to_string())?;
        let request = serde_json::from_str(request).map_err(|e| e.to_string())?;
        unsafe { &*engine }.request(request)
    }))
    .unwrap_or_else(|_| Err("Index operation panicked".into()));
    output(match result {
        Ok(value) => json!({"value":value}),
        Err(error) => json!({"error":error}),
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn em_cancel_search(engine: *const Engine) {
    if let Some(engine) = unsafe { engine.as_ref() } {
        engine.cancel_search();
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn em_close(engine: *mut Engine) {
    if !engine.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
            drop(Box::from_raw(engine));
        }));
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn em_string_free(string: *mut c_char) {
    if !string.is_null() {
        unsafe {
            drop(CString::from_raw(string));
        }
    }
}
