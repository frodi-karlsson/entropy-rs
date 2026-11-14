//! The C ABI that generated code calls (spec 5).
//!
//! One global runtime serves the whole process. Objects are boxed [`Obj`]s passed as raw
//! pointers. A function taking an object borrows it, except `ent_release`. A function returning
//! one returns a new reference.
//!
//! This is the only place a [`Halt`] becomes a process exit.

#![allow(unsafe_code)]

use std::ffi::{CString, c_char};
use std::io::{Stdout, Write};
use std::sync::Mutex;

use crate::obj::{Obj, RcError, Value};
use crate::{CharObj, Halt, Runtime, StrObj};

const ISSUES: &str = "https://github.com/frodi-karlsson/entropy-rs/issues";

struct State {
    rt: Runtime<Stdout>,
    /// `ENTROPY_DEBUG_RC=1`: freed objects are kept, and `ent_fini` reports the live count.
    debug_rc: bool,
    live: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// Sets up the runtime for the JIT, which takes the seed and fuel from `entropy run`'s flags.
pub fn init(rate: f32, seed: Option<u64>, fuel: Option<u64>) {
    let debug_rc = std::env::var_os("ENTROPY_DEBUG_RC").is_some_and(|v| v == "1");
    let rt = Runtime::new(rate, seed, fuel, std::io::stdout());
    *lock() = Some(State {
        rt,
        debug_rc,
        live: 0,
    });
}

fn lock() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs `f` on the runtime and turns a halt into an exit.
fn with<T>(f: impl FnOnce(&mut State) -> Result<T, Halt>) -> T {
    let mut guard = lock();
    let Some(state) = guard.as_mut() else {
        bug("runtime used before init");
    };
    match f(state) {
        Ok(v) => v,
        Err(Halt::OutOfFuel) => {
            // The exit status says what happened, even if the output is gone too.
            let _ = state.rt.flush();
            eprintln!("entropy: out of fuel");
            std::process::exit(4)
        }
        Err(Halt::OutputClosed) => std::process::exit(0),
    }
}

/// A broken invariant in generated code. Never caused by user input.
fn bug(what: &str) -> ! {
    eprintln!("entropy: internal error: {what}, please report it at {ISSUES}");
    std::process::abort()
}

fn rc(e: RcError) -> ! {
    match e {
        RcError::UseAfterFree => bug("object used after it was freed"),
        RcError::WrongKind => bug("object of the wrong kind"),
    }
}

fn new_obj(value: Value) -> *mut Obj {
    with(|s| {
        s.live += 1;
        Ok(())
    });
    Box::into_raw(Box::new(Obj::new(value)))
}

/// # Safety
/// `o` must come from this runtime and still be referenced by the caller.
unsafe fn obj<'a>(o: *mut Obj) -> &'a mut Obj {
    // SAFETY: the caller's reference keeps the box alive, and generated code is single-threaded.
    unsafe { o.as_mut() }.unwrap_or_else(|| bug("null object"))
}

/// The value of `name`, or `None` when unset. Exits with code 2 when it doesn't parse.
fn env_u64(name: &str) -> Option<u64> {
    match parse_env(name, std::env::var_os(name).as_deref()) {
        Ok(v) => v,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2)
        }
    }
}

fn parse_env(name: &str, value: Option<&std::ffi::OsStr>) -> Result<Option<u64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    value
        .to_str()
        .and_then(|v| v.parse().ok())
        .map(Some)
        .ok_or_else(|| {
            format!(
                "entropy: {name} must be a whole number from 0 to {}, got `{}`",
                u64::MAX,
                value.to_string_lossy()
            )
        })
}

unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
    fn fflush(stream: *mut std::ffi::c_void) -> i32;
}

const SIGPIPE: i32 = 13;
const SIG_IGN: usize = 1;

/// Sets up the runtime in a built executable: the rate is baked in, the seed and fuel come from
/// `ENTROPY_SEED` and `ENTROPY_FUEL`.
#[unsafe(no_mangle)]
pub extern "C" fn ent_init(rate: f32) {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("internal error in the Entropy runtime, please report it at {ISSUES}");
        eprintln!("{info}");
        std::process::exit(101);
    }));
    // A closed pipe should give EPIPE, as in a Rust `main`, so output ends with exit 0.
    // SAFETY: setting a signal's disposition to SIG_IGN has no other effect.
    unsafe { signal(SIGPIPE, SIG_IGN) };
    let seed = env_u64("ENTROPY_SEED");
    let fuel = env_u64("ENTROPY_FUEL");
    init(rate, seed, fuel);
}

/// Flushes output. In debug mode, prints the live object count.
#[unsafe(no_mangle)]
pub extern "C" fn ent_fini() {
    let mut guard = lock();
    let Some(state) = guard.as_mut() else {
        return;
    };
    let _ = state.rt.flush();
    if state.debug_rc {
        eprintln!("entropy: {} live objects", state.live);
    }
    let _ = std::io::stderr().flush();
}

#[unsafe(no_mangle)]
pub extern "C" fn ent_consume_tmp(mut v: f32) -> f32 {
    with(|s| s.rt.consume_real(&mut v))
}

#[unsafe(no_mangle)]
pub extern "C" fn ent_fmod(a: f32, b: f32) -> f32 {
    a % b
}

/// # Safety
/// `o` must be NULL or an object the caller holds a reference to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ent_retain(o: *mut Obj) {
    if o.is_null() {
        return;
    }
    // SAFETY: per this function's contract.
    unsafe { obj(o) }.retain().unwrap_or_else(|e| rc(e));
}

/// Drops the caller's reference.
///
/// # Safety
/// `o` must be NULL or an object the caller holds a reference to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ent_release(o: *mut Obj) {
    if o.is_null() {
        return;
    }
    // SAFETY: per this function's contract.
    let last = unsafe { obj(o) }.release().unwrap_or_else(|e| rc(e));
    if !last {
        return;
    }
    let debug_rc = with(|s| {
        s.live -= 1;
        Ok(s.debug_rc)
    });
    if !debug_rc {
        // SAFETY: this was the last reference, and the box came from `new_obj`.
        drop(unsafe { Box::from_raw(o) });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ent_char_new(v: f32) -> *mut Obj {
    new_obj(Value::Char(CharObj(v)))
}

/// # Safety
/// `o` must be a char object the caller holds a reference to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ent_char_consume(o: *mut Obj) -> f32 {
    // SAFETY: per this function's contract.
    let c = unsafe { obj(o) }.char_mut().unwrap_or_else(|e| rc(e));
    with(|s| s.rt.consume_char(c))
}

/// # Safety
/// `ptr` must point to `len` bytes of UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ent_str_lit(ptr: *const u8, len: usize) -> *mut Obj {
    // SAFETY: per this function's contract. Generated code passes a literal in read-only data.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    new_obj(Value::Str(StrObj::from_text(&String::from_utf8_lossy(
        bytes,
    ))))
}

/// # Safety
/// `o` must be a string object the caller holds a reference to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ent_str_consume(o: *mut Obj) {
    // SAFETY: per this function's contract.
    let st = unsafe { obj(o) }.str_mut().unwrap_or_else(|e| rc(e));
    with(|s| s.rt.consume_str(st));
}

/// # Safety
/// `o` must be a string object the caller holds a reference to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ent_str_snapshot(o: *mut Obj) -> *mut Obj {
    // SAFETY: per this function's contract.
    let snapshot = unsafe { obj(o) }
        .str_ref()
        .unwrap_or_else(|e| rc(e))
        .snapshot();
    new_obj(Value::Str(snapshot))
}

/// # Safety
/// `a` and `b` must be string objects the caller holds references to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ent_str_concat(a: *mut Obj, b: *mut Obj) -> *mut Obj {
    // SAFETY: per this function's contract. Both are only read, so they may be the same object.
    let (a, b) = unsafe { (&*obj(a), &*obj(b)) };
    let a = a.str_ref().unwrap_or_else(|e| rc(e));
    let b = b.str_ref().unwrap_or_else(|e| rc(e));
    new_obj(Value::Str(a.concat(b)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ent_str_from_real(v: f32) -> *mut Obj {
    new_obj(Value::Str(StrObj::from_real(v)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ent_str_from_char(v: f32) -> *mut Obj {
    new_obj(Value::Str(StrObj::from_char(v)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ent_print_real(v: f32) {
    with(|s| s.rt.print_real(v));
}

#[unsafe(no_mangle)]
pub extern "C" fn ent_print_char(v: f32) {
    with(|s| s.rt.print_char(v));
}

/// # Safety
/// `o` must be a string object the caller holds a reference to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ent_print_str(o: *mut Obj) {
    // SAFETY: per this function's contract.
    let st = unsafe { obj(o) }.str_ref().unwrap_or_else(|e| rc(e));
    with(|s| s.rt.print_str(st));
}

// Extern calls (spec 4.9).

/// The code point a char argument passes to C.
#[unsafe(no_mangle)]
pub extern "C" fn ent_char_to_i32(v: f32) -> i32 {
    crate::display_char(v) as i32
}

/// The displayed text of a string, cut at its first NUL, since C can't see past one.
fn c_string(s: &StrObj) -> CString {
    let text = s.text();
    let text = text.split('\0').next().unwrap_or_default();
    CString::new(text).unwrap_or_default()
}

/// A string argument for C, valid until `ent_cstr_free`.
///
/// # Safety
/// `o` must be a string object the caller holds a reference to.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ent_str_to_cstr(o: *mut Obj) -> *mut c_char {
    // SAFETY: per this function's contract.
    let st = unsafe { obj(o) }.str_ref().unwrap_or_else(|e| rc(e));
    c_string(st).into_raw()
}

/// # Safety
/// `p` must be NULL or come from `ent_str_to_cstr`, and not be freed already.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ent_cstr_free(p: *mut c_char) {
    if !p.is_null() {
        // SAFETY: per this function's contract.
        drop(unsafe { CString::from_raw(p) });
    }
}

/// Flushes our output before C writes its own.
#[unsafe(no_mangle)]
pub extern "C" fn ent_flush() {
    with(|s| s.rt.flush());
}

/// Flushes whatever C wrote through stdio before we print again.
#[unsafe(no_mangle)]
pub extern "C" fn ent_after_extern() {
    // SAFETY: fflush(NULL) flushes every open stdio stream and has no other effect.
    unsafe { fflush(std::ptr::null_mut()) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn char_arguments() {
        assert_eq!(ent_char_to_i32(65.0), 65);
        assert_eq!(ent_char_to_i32(65.4), 65);
        assert_eq!(ent_char_to_i32(f32::NAN), 0xFFFD);
        assert_eq!(ent_char_to_i32(-1.0), 0xFFFD);
        assert_eq!(ent_char_to_i32(55_296.0), 0xFFFD);
    }

    #[test]
    fn string_arguments_stop_at_nul() {
        let c = |s: &str| c_string(&StrObj::from_text(s));
        assert_eq!(c("hé"), CString::new("hé").unwrap());
        assert_eq!(c("a\0b"), CString::new("a").unwrap());
        assert_eq!(c(""), CString::new("").unwrap());
        // A cell that drifted to 0.4 displays as NUL too.
        assert_eq!(
            c_string(&StrObj(vec![104.0, 0.4, 105.0])),
            CString::new("h").unwrap()
        );
    }

    #[test]
    fn env_values() {
        assert_eq!(parse_env("ENTROPY_SEED", None), Ok(None));
        assert_eq!(
            parse_env("ENTROPY_SEED", Some(OsStr::new("42"))),
            Ok(Some(42))
        );
        assert_eq!(
            parse_env("ENTROPY_FUEL", Some(OsStr::new("-1"))),
            Err(
                "entropy: ENTROPY_FUEL must be a whole number from 0 to 18446744073709551615, \
                 got `-1`"
                    .to_string()
            )
        );
        assert!(parse_env("ENTROPY_SEED", Some(OsStr::new(""))).is_err());
    }
}
