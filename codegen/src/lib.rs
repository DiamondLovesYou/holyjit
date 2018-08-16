extern crate mmap;
extern crate region;
extern crate cranelift_codegen as codegen;
extern crate cranelift_frontend as frontend;
extern crate cranelift_native as native;
extern crate holyjit_lir as lir;

mod lower;
mod exec_alloc;
pub mod error;

use codegen::settings::Configurable;
use exec_alloc::{WrittableCode, ExecutableCode};

/// This is a code generator context, which is used to lower a LIR Unit into
/// machine code.
pub struct Context {
    ctx: codegen::Context,
    isa: Box<codegen::isa::TargetIsa>,
}

/// Result of a compiled lir::Unit.
pub struct JitCode {
    code: ExecutableCode,
}

impl Context {
    /// Create a lowering (code generator and executable page allocation)
    /// context for the architecture on which this code is running.
    pub fn new() -> Self {
        // Extract configuration builders tuned for the architecture on which
        // this code is running.
        let (mut settings_bld, isa_bld) = native::builders().unwrap();
        // Optimize for compilation time.
        settings_bld.set("opt_level", "fastest").unwrap();
        // Check the emitted Cranelift IR.
        settings_bld.set("enable_verifier", "1").unwrap();
        // Use Rust call convention.
        settings_bld.set("call_conv", "system_v").unwrap();
        // Generate position independent code.
        settings_bld.set("is_pic", "1").unwrap();
        // No need to generate a single return per function.
        settings_bld.set("return_at_end", "0").unwrap();
        // Do not attempt to avoid trap on divisions. (TODO: double check that
        // this is what rust expects)
        settings_bld.set("avoid_div_traps", "0").unwrap();
        let flags = codegen::settings::Flags::new(settings_bld);
        let isa = isa_bld.finish(flags);

        Self {
            ctx: codegen::Context::new(),
            isa: isa,
        }
    }

    /// Given an HolyJIT LIR Unit, convert it to a Cranelift function in order
    /// to generate the corresponding bytes, then allocate memory pages and map
    /// them as executable.
    pub fn compile(&mut self, unit: &lir::Unit) -> error::LowerResult<JitCode> {
        let &mut Context { ref mut ctx, ref isa, .. } = self;
        ctx.func = lower::convert(unit)?;
        let mut reloc_sink = exec_alloc::NullRelocSink {};
        let mut trap_sink = exec_alloc::NullTrapSink {};
        let code_size = ctx.compile(isa.as_ref())?;
        let mut code = WrittableCode::with_capacity(code_size as usize)?;
        unsafe {
            ctx.emit_to_memory(isa.as_ref(), code.as_ptr(), &mut reloc_sink, &mut trap_sink);
        }
        let code = code.make_executable(reloc_sink, trap_sink)?;
        Ok(JitCode { code })
    }
}

impl JitCode {
    pub fn as_ptr(&self) -> *const u8 {
        self.code.as_ptr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn check_create_context() {
        let _ctx = Context::new();
        assert!(true);
    }

    #[test]
    fn check_add1_unit() {
        let mut ctx = Context::new();
        let simple_unit = lir::Unit::new(lir::UnitId::Function(0));
        let code = ctx.compile(&simple_unit).unwrap();
        let add1 : fn(i32) -> i32 = unsafe {
            mem::transmute(code.as_ptr())
        };
        assert_eq!(add1(12), 13);
    }
}
