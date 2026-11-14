//! HIR to Cranelift IR (spec 6). One lowering, shared by the JIT and AOT.
//!
//! Every variable and literal is a Cranelift variable: an f32 for a real, an object pointer for a
//! char or string. Generated code never decays a value itself. It calls the runtime for every
//! step.

use cranelift_codegen::ir::condcodes::FloatCC;
use cranelift_codegen::ir::{AbiParam, Block, InstBuilder, Type, UserFuncName, Value, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use entropy_sema::hir::{self, ArithOp, CmpOp, Cond, Expr, ExprKind, Literal, Program, Stmt};

use crate::Error;

/// A runtime C ABI function that generated code calls.
#[derive(Clone, Copy, Debug)]
pub enum Rt {
    Init,
    Fini,
    ConsumeTmp,
    Fmod,
    Retain,
    Release,
    CharNew,
    CharConsume,
    StrLit,
    StrConsume,
    StrSnapshot,
    StrConcat,
    StrFromReal,
    StrFromChar,
    PrintReal,
    PrintChar,
    PrintStr,
    CharToI32,
    StrToCstr,
    CstrFree,
    Flush,
    AfterExtern,
}

/// A parameter or return type: an f32, an i32, or a pointer-sized integer (an object or a length).
#[derive(Clone, Copy)]
enum Ty {
    F,
    I,
    P,
}

impl Rt {
    pub const ALL: [Rt; 22] = [
        Rt::Init,
        Rt::Fini,
        Rt::ConsumeTmp,
        Rt::Fmod,
        Rt::Retain,
        Rt::Release,
        Rt::CharNew,
        Rt::CharConsume,
        Rt::StrLit,
        Rt::StrConsume,
        Rt::StrSnapshot,
        Rt::StrConcat,
        Rt::StrFromReal,
        Rt::StrFromChar,
        Rt::PrintReal,
        Rt::PrintChar,
        Rt::PrintStr,
        Rt::CharToI32,
        Rt::StrToCstr,
        Rt::CstrFree,
        Rt::Flush,
        Rt::AfterExtern,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Rt::Init => "ent_init",
            Rt::Fini => "ent_fini",
            Rt::ConsumeTmp => "ent_consume_tmp",
            Rt::Fmod => "ent_fmod",
            Rt::Retain => "ent_retain",
            Rt::Release => "ent_release",
            Rt::CharNew => "ent_char_new",
            Rt::CharConsume => "ent_char_consume",
            Rt::StrLit => "ent_str_lit",
            Rt::StrConsume => "ent_str_consume",
            Rt::StrSnapshot => "ent_str_snapshot",
            Rt::StrConcat => "ent_str_concat",
            Rt::StrFromReal => "ent_str_from_real",
            Rt::StrFromChar => "ent_str_from_char",
            Rt::PrintReal => "ent_print_real",
            Rt::PrintChar => "ent_print_char",
            Rt::PrintStr => "ent_print_str",
            Rt::CharToI32 => "ent_char_to_i32",
            Rt::StrToCstr => "ent_str_to_cstr",
            Rt::CstrFree => "ent_cstr_free",
            Rt::Flush => "ent_flush",
            Rt::AfterExtern => "ent_after_extern",
        }
    }

    fn types(self) -> (&'static [Ty], &'static [Ty]) {
        use Ty::{F, I, P};
        match self {
            Rt::Init | Rt::PrintReal | Rt::PrintChar => (&[F], &[]),
            Rt::Fini | Rt::Flush | Rt::AfterExtern => (&[], &[]),
            Rt::ConsumeTmp => (&[F], &[F]),
            Rt::Fmod => (&[F, F], &[F]),
            Rt::Retain | Rt::Release | Rt::StrConsume | Rt::PrintStr | Rt::CstrFree => (&[P], &[]),
            Rt::CharNew | Rt::StrFromReal | Rt::StrFromChar => (&[F], &[P]),
            Rt::CharConsume => (&[P], &[F]),
            Rt::StrLit | Rt::StrConcat => (&[P, P], &[P]),
            Rt::StrSnapshot | Rt::StrToCstr => (&[P], &[P]),
            Rt::CharToI32 => (&[F], &[I]),
        }
    }

    /// Declares this function as an import of `module`.
    pub fn declare<M: Module>(self, module: &mut M) -> Result<FuncId, Error> {
        let ptr = module.target_config().pointer_type();
        let ty = |t: &Ty| {
            AbiParam::new(match t {
                Ty::F => types::F32,
                Ty::I => types::I32,
                Ty::P => ptr,
            })
        };
        let (params, returns) = self.types();
        let mut sig = module.make_signature();
        sig.params.extend(params.iter().map(ty));
        sig.returns.extend(returns.iter().map(ty));
        Ok(module.declare_function(self.name(), Linkage::Import, &sig)?)
    }
}

/// The lowered `entropy_main`.
pub struct Lowered {
    pub main: FuncId,
    /// The function's IR as text, for `--emit clif`.
    pub clif: String,
    /// The C symbols of the externs the program calls, which the JIT must resolve.
    pub externs: Vec<String>,
}

/// Defines `entropy_main() -> void` in `module`.
pub fn lower<M: Module>(module: &mut M, program: &Program) -> Result<Lowered, Error> {
    let config = module.target_config();
    let ptr = config.pointer_type();
    let rt_ids = Rt::ALL
        .iter()
        .map(|rt| rt.declare(module))
        .collect::<Result<Vec<_>, _>>()?;

    // Only externs that are called are imported. The linker ignores the rest, so the JIT must too.
    let mut called = vec![false; program.externs.len()];
    calls_in(&program.body, &mut called);
    let mut extern_ids = Vec::new();
    let mut symbols = Vec::new();
    for (ext, called) in program.externs.iter().zip(called) {
        if !called {
            extern_ids.push(None);
            continue;
        }
        let sig = extern_signature(module, ext);
        extern_ids.push(Some(module.declare_function(
            &ext.symbol,
            Linkage::Import,
            &sig,
        )?));
        symbols.push(ext.symbol.clone());
    }

    // String literal bytes, NUL-terminated so even an empty one has an address.
    let mut strings = Vec::new();
    for literal in &program.literals {
        if let Literal::Str(s) = literal {
            let id = module.declare_anonymous_data(false, false)?;
            let mut data = DataDescription::new();
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0);
            data.define(bytes.into_boxed_slice());
            module.define_data(id, &data)?;
            strings.push(id);
        }
    }

    let main =
        module.declare_function("entropy_main", Linkage::Export, &module.make_signature())?;
    let mut ctx = module.make_context();
    ctx.func.name = UserFuncName::user(0, main.as_u32());
    let mut fctx = FunctionBuilderContext::new();
    let mut b = FunctionBuilder::new(&mut ctx.func, &mut fctx);
    let rt = rt_ids
        .iter()
        .map(|&id| module.declare_func_in_func(id, b.func))
        .collect();
    let externs = extern_ids
        .iter()
        .map(|id| id.map(|id| module.declare_func_in_func(id, b.func)))
        .collect();
    let strings: Vec<_> = strings
        .iter()
        .map(|&id| module.declare_data_in_func(id, b.func))
        .collect();

    let entry = b.create_block();
    b.switch_to_block(entry);
    let mut l = Lower {
        b,
        rt,
        externs,
        params: program.externs.iter().map(|e| e.params.clone()).collect(),
        ptr,
        vars: Vec::new(),
        lits: Vec::new(),
    };

    for var in &program.vars {
        let v = l.var_of(var.ty);
        let init = if var.ty == hir::Type::Real {
            l.b.ins().f32const(0.0)
        } else {
            l.null()
        };
        l.b.def_var(v, init);
        l.vars.push(v);
    }
    let mut strings = strings.into_iter();
    for literal in &program.literals {
        let (v, init) = match literal {
            Literal::Real(x) => (l.b.declare_var(types::F32), l.b.ins().f32const(*x)),
            Literal::Char(c) => {
                let code = l.b.ins().f32const(*c as u32 as f32);
                (l.b.declare_var(ptr), l.call(Rt::CharNew, &[code]))
            }
            Literal::Str(s) => {
                let gv = strings
                    .next()
                    .ok_or(Error::Internal("missing string data"))?;
                let addr = l.b.ins().symbol_value(ptr, gv);
                let len = l.b.ins().iconst(ptr, s.len() as i64);
                (l.b.declare_var(ptr), l.call(Rt::StrLit, &[addr, len]))
            }
        };
        l.b.def_var(v, init);
        l.lits.push(v);
    }

    l.block(program, &program.body)?;

    for (literal, v) in program.literals.iter().zip(l.lits.clone()) {
        if !matches!(literal, Literal::Real(_)) {
            let p = l.b.use_var(v);
            l.call0(Rt::Release, &[p]);
        }
    }
    l.b.ins().return_(&[]);
    l.b.seal_all_blocks();
    l.b.finalize(config);

    let clif = ctx.func.display().to_string();
    module.define_function(main, &mut ctx)?;
    Ok(Lowered {
        main,
        clif,
        externs: symbols,
    })
}

/// The C signature of an extern (spec 4.9).
fn extern_signature<M: Module>(module: &M, ext: &hir::Extern) -> cranelift_codegen::ir::Signature {
    let ptr = module.target_config().pointer_type();
    let c_type = |ty: &hir::Type| {
        AbiParam::new(match ty {
            hir::Type::Real => types::F64,
            hir::Type::Char => types::I32,
            hir::Type::String => ptr,
        })
    };
    let mut sig = module.make_signature();
    sig.params.extend(ext.params.iter().map(c_type));
    sig.returns.extend(ext.returns.iter().map(c_type));
    sig
}

/// Marks every extern called in `block`.
fn calls_in(block: &hir::Block, called: &mut [bool]) {
    fn expr(e: &Expr, called: &mut [bool]) {
        match &e.kind {
            ExprKind::Var(_) | ExprKind::Lit(_) => {}
            ExprKind::Arith { lhs, rhs, .. } | ExprKind::Concat { lhs, rhs } => {
                expr(lhs, called);
                expr(rhs, called);
            }
            ExprKind::Call(c) => call(c, called),
        }
    }
    fn call(c: &hir::Call, called: &mut [bool]) {
        if let Some(flag) = called.get_mut(c.callee.0) {
            *flag = true;
        }
        for arg in &c.args {
            expr(arg, called);
        }
    }
    for stmt in &block.stmts {
        match stmt {
            Stmt::Declare(_) => {}
            Stmt::Assign { value, .. } | Stmt::Print(value) => expr(value, called),
            Stmt::If { cond, body } | Stmt::While { cond, body } => {
                expr(&cond.lhs, called);
                expr(&cond.rhs, called);
                calls_in(body, called);
            }
            Stmt::Call(c) => call(c, called),
        }
    }
}

/// A value after its consumption step. Objects are owned references.
#[derive(Clone, Copy)]
enum Val {
    Real(Value),
    /// The object and its consumed value.
    Char(Value, Value),
    Str(Value),
}

struct Lower<'a> {
    b: FunctionBuilder<'a>,
    rt: Vec<cranelift_codegen::ir::FuncRef>,
    /// By extern id: the import, or `None` if it is never called, and the parameter types.
    externs: Vec<Option<cranelift_codegen::ir::FuncRef>>,
    params: Vec<Vec<hir::Type>>,
    ptr: Type,
    vars: Vec<Variable>,
    lits: Vec<Variable>,
}

impl Lower<'_> {
    fn var_of(&mut self, ty: hir::Type) -> Variable {
        let ty = if ty == hir::Type::Real {
            types::F32
        } else {
            self.ptr
        };
        self.b.declare_var(ty)
    }

    fn null(&mut self) -> Value {
        self.b.ins().iconst(self.ptr, 0)
    }

    /// Calls a runtime function that returns a value.
    fn call(&mut self, f: Rt, args: &[Value]) -> Value {
        let inst = self.b.ins().call(self.rt[f as usize], args);
        self.b.inst_results(inst)[0]
    }

    fn call0(&mut self, f: Rt, args: &[Value]) {
        self.b.ins().call(self.rt[f as usize], args);
    }

    fn release(&mut self, p: Value) {
        self.call0(Rt::Release, &[p]);
    }

    /// Releases what a char or string variable holds and leaves it NULL.
    fn clear(&mut self, program: &Program, id: hir::VarId) -> Result<(), Error> {
        let var = program
            .vars
            .get(id.0)
            .ok_or(Error::Internal("unknown variable"))?;
        if var.ty != hir::Type::Real {
            let v = self.vars[id.0];
            let p = self.b.use_var(v);
            self.release(p);
            let null = self.null();
            self.b.def_var(v, null);
        }
        Ok(())
    }

    fn block(&mut self, program: &Program, block: &hir::Block) -> Result<(), Error> {
        for stmt in &block.stmts {
            self.stmt(program, stmt)?;
        }
        for &id in &block.vars {
            self.clear(program, id)?;
        }
        Ok(())
    }

    fn stmt(&mut self, program: &Program, stmt: &Stmt) -> Result<(), Error> {
        match stmt {
            Stmt::Declare(id) => self.clear(program, *id)?,
            Stmt::Assign { var, value } => {
                let target = program
                    .vars
                    .get(var.0)
                    .ok_or(Error::Internal("unknown variable"))?
                    .ty;
                let v = self.vars[var.0];
                let value = self.consume(value)?;
                let new = match (target, value) {
                    (hir::Type::Real, Val::Real(x)) => {
                        self.b.def_var(v, x);
                        return Ok(());
                    }
                    (hir::Type::Real, Val::Char(p, x)) => {
                        self.release(p);
                        self.b.def_var(v, x);
                        return Ok(());
                    }
                    (hir::Type::Char, Val::Real(x)) => self.call(Rt::CharNew, &[x]),
                    (hir::Type::Char, Val::Char(p, _)) | (hir::Type::String, Val::Str(p)) => p,
                    _ => return Err(Error::Internal("assignment type mismatch")),
                };
                // The slot takes over the value's reference. Then the old one is dropped.
                let old = self.b.use_var(v);
                self.b.def_var(v, new);
                self.release(old);
            }
            Stmt::Print(e) => match self.consume(e)? {
                Val::Real(x) => self.call0(Rt::PrintReal, &[x]),
                Val::Char(p, x) => {
                    self.call0(Rt::PrintChar, &[x]);
                    self.release(p);
                }
                Val::Str(p) => {
                    self.call0(Rt::PrintStr, &[p]);
                    self.release(p);
                }
            },
            Stmt::If { cond, body } => {
                let then = self.b.create_block();
                let done = self.b.create_block();
                let c = self.cond(cond)?;
                self.b.ins().brif(c, then, &[], done, &[]);
                self.switch(then);
                self.block(program, body)?;
                self.b.ins().jump(done, &[]);
                self.switch(done);
            }
            Stmt::While { cond, body } => {
                let head = self.b.create_block();
                let then = self.b.create_block();
                let done = self.b.create_block();
                self.b.ins().jump(head, &[]);
                self.switch(head);
                let c = self.cond(cond)?;
                self.b.ins().brif(c, then, &[], done, &[]);
                self.switch(then);
                self.block(program, body)?;
                self.b.ins().jump(head, &[]);
                self.switch(done);
            }
            // The result, if any, is discarded without being consumed.
            Stmt::Call(c) => {
                self.extern_call(c)?;
            }
        }
        Ok(())
    }

    /// Calls an extern and returns its raw C result (spec 4.9).
    fn extern_call(&mut self, c: &hir::Call) -> Result<Option<Value>, Error> {
        let params = self
            .params
            .get(c.callee.0)
            .cloned()
            .ok_or(Error::Internal("unknown extern"))?;
        let callee = self
            .externs
            .get(c.callee.0)
            .copied()
            .flatten()
            .ok_or(Error::Internal("extern not imported"))?;
        // Each argument is consumed and converted before the next one is evaluated (4.3).
        let mut args = Vec::new();
        let mut c_strings = Vec::new();
        for (arg, param) in c.args.iter().zip(&params) {
            match param {
                hir::Type::Real => {
                    let x = self.number(arg)?;
                    args.push(self.b.ins().fpromote(types::F64, x));
                }
                hir::Type::Char => {
                    let x = self.number(arg)?;
                    args.push(self.call(Rt::CharToI32, &[x]));
                }
                hir::Type::String => {
                    let Val::Str(p) = self.consume(arg)? else {
                        return Err(Error::Internal("non-string string argument"));
                    };
                    let s = self.call(Rt::StrToCstr, &[p]);
                    self.release(p);
                    args.push(s);
                    c_strings.push(s);
                }
            }
        }
        self.call0(Rt::Flush, &[]);
        let inst = self.b.ins().call(callee, &args);
        let result = self.b.inst_results(inst).first().copied();
        self.call0(Rt::AfterExtern, &[]);
        for s in c_strings {
            self.call0(Rt::CstrFree, &[s]);
        }
        Ok(result)
    }

    fn switch(&mut self, block: Block) {
        self.b.switch_to_block(block);
    }

    fn cond(&mut self, cond: &Cond) -> Result<Value, Error> {
        let a = self.number(&cond.lhs)?;
        let b = self.number(&cond.rhs)?;
        // Ordered comparisons, each used directly, so NaN makes every one false.
        let cc = match cond.op {
            CmpOp::Lt => FloatCC::LessThan,
            CmpOp::Gt => FloatCC::GreaterThan,
            CmpOp::Le => FloatCC::LessThanOrEqual,
            CmpOp::Ge => FloatCC::GreaterThanOrEqual,
        };
        Ok(self.b.ins().fcmp(cc, a, b))
    }

    /// Consumes a real or char and returns its value.
    fn number(&mut self, e: &Expr) -> Result<Value, Error> {
        match self.consume(e)? {
            Val::Real(x) => Ok(x),
            Val::Char(p, x) => {
                self.release(p);
                Ok(x)
            }
            Val::Str(_) => Err(Error::Internal("string used as a number")),
        }
    }

    /// Consumes an operand of concatenation and fixes its value as a new string (spec 4.1).
    fn snapshot(&mut self, e: &Expr) -> Result<Value, Error> {
        Ok(match self.consume(e)? {
            Val::Real(x) => self.call(Rt::StrFromReal, &[x]),
            Val::Char(p, x) => {
                self.release(p);
                self.call(Rt::StrFromChar, &[x])
            }
            Val::Str(p) => {
                let s = self.call(Rt::StrSnapshot, &[p]);
                self.release(p);
                s
            }
        })
    }

    /// Evaluates an expression and applies its consumption step (spec 4.3).
    fn consume(&mut self, e: &Expr) -> Result<Val, Error> {
        let v = match &e.kind {
            ExprKind::Var(id) => self.vars[id.0],
            ExprKind::Lit(id) => self.lits[id.0],
            ExprKind::Arith { op, lhs, rhs } => {
                let a = self.number(lhs)?;
                let b = self.number(rhs)?;
                let x = match op {
                    ArithOp::Add => self.b.ins().fadd(a, b),
                    ArithOp::Sub => self.b.ins().fsub(a, b),
                    ArithOp::Mul => self.b.ins().fmul(a, b),
                    ArithOp::Div => self.b.ins().fdiv(a, b),
                    ArithOp::Rem => self.call(Rt::Fmod, &[a, b]),
                };
                if e.ty == hir::Type::Char {
                    let p = self.call(Rt::CharNew, &[x]);
                    let x = self.call(Rt::CharConsume, &[p]);
                    return Ok(Val::Char(p, x));
                }
                return Ok(Val::Real(self.call(Rt::ConsumeTmp, &[x])));
            }
            ExprKind::Concat { lhs, rhs } => {
                let a = self.snapshot(lhs)?;
                let b = self.snapshot(rhs)?;
                let s = self.call(Rt::StrConcat, &[a, b]);
                self.release(a);
                self.release(b);
                self.call0(Rt::StrConsume, &[s]);
                return Ok(Val::Str(s));
            }
            ExprKind::Call(c) => {
                let result = self
                    .extern_call(c)?
                    .ok_or(Error::Internal("void call used as a value"))?;
                // A return value is a temporary, consumed by its parent like an operator's result.
                return Ok(match e.ty {
                    hir::Type::Real => {
                        let x = self.b.ins().fdemote(types::F32, result);
                        Val::Real(self.call(Rt::ConsumeTmp, &[x]))
                    }
                    hir::Type::Char => {
                        let x = self.b.ins().fcvt_from_sint(types::F32, result);
                        let p = self.call(Rt::CharNew, &[x]);
                        Val::Char(p, self.call(Rt::CharConsume, &[p]))
                    }
                    hir::Type::String => return Err(Error::Internal("string returned by extern")),
                });
            }
        };
        Ok(match e.ty {
            hir::Type::Real => {
                let x = self.b.use_var(v);
                let x = self.call(Rt::ConsumeTmp, &[x]);
                self.b.def_var(v, x);
                Val::Real(x)
            }
            hir::Type::Char => {
                let p = self.b.use_var(v);
                self.call0(Rt::Retain, &[p]);
                Val::Char(p, self.call(Rt::CharConsume, &[p]))
            }
            hir::Type::String => {
                let p = self.b.use_var(v);
                self.call0(Rt::Retain, &[p]);
                self.call0(Rt::StrConsume, &[p]);
                Val::Str(p)
            }
        })
    }
}
