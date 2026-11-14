//! In-process compilation for `entropy run`.

#![allow(unsafe_code)]

use std::path::PathBuf;

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::default_libcall_names;
use entropy_runtime::capi;
use entropy_sema::hir::Program;
use libloading::os::unix::Library;

use crate::{Error, Rt, extern_symbols, lower};

/// Compiles and runs a program in this process. Running out of fuel or losing stdout exits the
/// process (spec 5). Externs are looked up in `dylibs`, then in this process, then in libm.
pub fn run(
    program: &Program,
    rate: f32,
    seed: Option<u64>,
    fuel: Option<u64>,
    dylibs: &[PathBuf],
) -> Result<(), Error> {
    let libraries = open(dylibs)?;
    let mut symbols = Vec::new();
    for name in extern_symbols(program) {
        let address = libraries
            .iter()
            // SAFETY: only the address is read. The call's types come from the extern declaration.
            .find_map(|lib| unsafe { lib.get::<*const u8>(&name) }.ok().map(|s| *s))
            .ok_or_else(|| Error::MissingSymbol(name.clone()))?;
        symbols.push((name, address));
    }
    let mut module = module(&symbols)?;
    let main = lower(&mut module, program)?.main;
    module.finalize_definitions()?;
    let code = module.get_finalized_function(main);
    // SAFETY: `entropy_main` was defined with the `() -> void` C signature.
    let entropy_main = unsafe { std::mem::transmute::<*const u8, extern "C" fn()>(code) };
    capi::init(rate, seed, fuel);
    entropy_main();
    capi::ent_fini();
    // The libraries stay open until here, because the program calls into them.
    drop(libraries);
    Ok(())
}

/// The libraries to search, in order.
fn open(dylibs: &[PathBuf]) -> Result<Vec<Library>, Error> {
    let mut libraries = Vec::new();
    for path in dylibs {
        // A bare file name would make dlopen search the system paths instead of this directory.
        let path = std::path::absolute(path).unwrap_or_else(|_| path.clone());
        // SAFETY: loading a library runs its initialisers. The user asked for this one.
        let lib = unsafe { Library::new(&path) }
            .map_err(|e| Error::Dylib(path.clone(), e.to_string()))?;
        libraries.push(lib);
    }
    libraries.push(Library::this());
    // On Linux the `entropy` binary may not link libm at all.
    if cfg!(target_os = "linux") {
        // SAFETY: libm has no initialisers with side effects.
        if let Ok(lib) = unsafe { Library::new("libm.so.6") } {
            libraries.push(lib);
        }
    }
    Ok(libraries)
}

/// A JIT module that can call every runtime function and the given externs.
pub fn module(externs: &[(String, *const u8)]) -> Result<JITModule, Error> {
    let mut builder = JITBuilder::with_flags(&[("opt_level", "speed")], default_libcall_names())?;
    for rt in Rt::ALL {
        builder.symbol(rt.name(), address(rt));
    }
    for (name, address) in externs {
        builder.symbol(name, *address);
    }
    Ok(JITModule::new(builder))
}

fn address(rt: Rt) -> *const u8 {
    match rt {
        Rt::Init => capi::ent_init as *const u8,
        Rt::Fini => capi::ent_fini as *const u8,
        Rt::ConsumeTmp => capi::ent_consume_tmp as *const u8,
        Rt::Fmod => capi::ent_fmod as *const u8,
        Rt::Retain => capi::ent_retain as *const u8,
        Rt::Release => capi::ent_release as *const u8,
        Rt::CharNew => capi::ent_char_new as *const u8,
        Rt::CharConsume => capi::ent_char_consume as *const u8,
        Rt::StrLit => capi::ent_str_lit as *const u8,
        Rt::StrConsume => capi::ent_str_consume as *const u8,
        Rt::StrSnapshot => capi::ent_str_snapshot as *const u8,
        Rt::StrConcat => capi::ent_str_concat as *const u8,
        Rt::StrFromReal => capi::ent_str_from_real as *const u8,
        Rt::StrFromChar => capi::ent_str_from_char as *const u8,
        Rt::PrintReal => capi::ent_print_real as *const u8,
        Rt::PrintChar => capi::ent_print_char as *const u8,
        Rt::PrintStr => capi::ent_print_str as *const u8,
        Rt::CharToI32 => capi::ent_char_to_i32 as *const u8,
        Rt::StrToCstr => capi::ent_str_to_cstr as *const u8,
        Rt::CstrFree => capi::ent_cstr_free as *const u8,
        Rt::Flush => capi::ent_flush as *const u8,
        Rt::AfterExtern => capi::ent_after_extern as *const u8,
    }
}
