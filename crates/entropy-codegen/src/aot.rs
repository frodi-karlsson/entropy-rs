//! Object files for `entropy build`.

use cranelift_codegen::ir::{AbiParam, InstBuilder, UserFuncName, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use entropy_sema::hir::Program;

use crate::{Error, Rt, lower};

pub struct Object {
    pub bytes: Vec<u8>,
    /// The IR of every function, for `--emit clif`.
    pub clif: String,
}

/// Compiles a program to an object file for the host, with a C `main` that runs it at `rate`.
pub fn object(program: &Program, rate: f32) -> Result<Object, Error> {
    let host = |e: &dyn std::fmt::Display| Error::Host(e.to_string());
    let mut flags = settings::builder();
    // Linux `cc` builds position-independent executables by default.
    flags.set("is_pic", "true").map_err(|e| host(&e))?;
    flags.set("opt_level", "speed").map_err(|e| host(&e))?;
    let isa = cranelift_native::builder()
        .map_err(|e| host(&e))?
        .finish(settings::Flags::new(flags))
        .map_err(|e| host(&e))?;
    let mut module =
        ObjectModule::new(ObjectBuilder::new(isa, "entropy", default_libcall_names())?);

    let lowered = lower(&mut module, program)?;
    let init = Rt::Init.declare(&mut module)?;
    let fini = Rt::Fini.declare(&mut module)?;

    let ptr = module.target_config().pointer_type();
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I32));
    sig.params.push(AbiParam::new(ptr));
    sig.returns.push(AbiParam::new(types::I32));
    let main = module.declare_function("main", Linkage::Export, &sig)?;

    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    ctx.func.name = UserFuncName::user(0, main.as_u32());
    let mut fctx = FunctionBuilderContext::new();
    let mut b = FunctionBuilder::new(&mut ctx.func, &mut fctx);
    let init = module.declare_func_in_func(init, b.func);
    let fini = module.declare_func_in_func(fini, b.func);
    let entropy_main = module.declare_func_in_func(lowered.main, b.func);
    let entry = b.create_block();
    b.append_block_params_for_function_params(entry);
    b.switch_to_block(entry);
    let rate = b.ins().f32const(rate);
    b.ins().call(init, &[rate]);
    b.ins().call(entropy_main, &[]);
    b.ins().call(fini, &[]);
    let zero = b.ins().iconst(types::I32, 0);
    b.ins().return_(&[zero]);
    b.seal_all_blocks();
    b.finalize(module.target_config());

    let clif = format!("{}\n{}", lowered.clif, ctx.func.display());
    module.define_function(main, &mut ctx)?;
    let bytes = module.finish().emit().map_err(|e| host(&e))?;
    Ok(Object { bytes, clif })
}
