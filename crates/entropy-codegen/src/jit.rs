//! In-process compilation for `entropy run`.

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::default_libcall_names;
use entropy_runtime::capi;

use crate::{Error, Rt};

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
    }
}
