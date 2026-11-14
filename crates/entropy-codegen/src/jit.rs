//! In-process compilation for `entropy run`.

#![allow(unsafe_code)]

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::default_libcall_names;
use entropy_runtime::capi;
use entropy_sema::hir::Program;

use crate::{Error, Rt, lower};

/// Compiles and runs a program in this process. Running out of fuel or losing stdout exits the
/// process (spec 5).
pub fn run(
    program: &Program,
    rate: f32,
    seed: Option<u64>,
    fuel: Option<u64>,
) -> Result<(), Error> {
    // Symbol resolution for externs arrives with `--dylib`. Until then cranelift-jit would panic
    // on a symbol it can't find.
    if !program.externs.is_empty() {
        return Err(Error::Externs);
    }
    let mut module = module()?;
    let main = lower(&mut module, program)?.main;
    module.finalize_definitions()?;
    let code = module.get_finalized_function(main);
    // SAFETY: `entropy_main` was defined with the `() -> void` C signature.
    let entropy_main = unsafe { std::mem::transmute::<*const u8, extern "C" fn()>(code) };
    capi::init(rate, seed, fuel);
    entropy_main();
    capi::ent_fini();
    Ok(())
}

/// A JIT module that can call every runtime function.
pub fn module() -> Result<JITModule, Error> {
    let mut builder = JITBuilder::with_flags(&[("opt_level", "speed")], default_libcall_names())?;
    for rt in Rt::ALL {
        builder.symbol(rt.name(), address(rt));
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
