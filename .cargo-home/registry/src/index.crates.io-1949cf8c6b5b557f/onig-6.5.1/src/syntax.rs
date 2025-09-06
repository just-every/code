#![allow(clippy::transmute_ptr_to_ptr)]
#![allow(clippy::transmute_ptr_to_ref)]

use super::{MetaCharType, RegexOptions, SyntaxBehavior, SyntaxOperator};
use std::mem::transmute;

/// Meta Character State
///
/// Defines if a given meta character is enabled or not within a given
/// syntax. If the character is enabled it also contains the rust
/// `char` that it is set to.
#[derive(Copy, Clone)]
pub enum MetaChar {
    /// The meta character is set to the chosen `char`
    Character(char),
    /// The meta character is not enabled
    Ineffective,
}

/// Onig Syntax Wrapper
///
/// Each syntax dfines a flavour of regex syntax. This type allows
/// interaction with the built-in syntaxes through the static accessor
/// functions (`Syntax::emacs()`, `Syntax::default()` etc.) and the
/// creation of custom syntaxes.
///
/// For a demonstration of creating a custom syntax see
/// `examples/syntax.rs` in the main onig crate.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Syntax {
    raw: onig_sys::OnigSyntaxType,
}

impl Syntax {
    /// Python syntax
    pub fn python() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxPython) }
    }
    /// Plain text syntax
    pub fn asis() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxASIS) }
    }

    /// POSIX Basic RE syntax
    pub fn posix_basic() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxPosixBasic) }
    }

    /// POSIX Extended RE syntax
    pub fn posix_extended() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxPosixExtended) }
    }

    /// Emacs syntax
    pub fn emacs() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxEmacs) }
    }

    /// Grep syntax
    pub fn grep() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxGrep) }
    }

    /// GNU regex syntax
    pub fn gnu_regex() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxGnuRegex) }
    }

    /// Java (Sun java.util.regex) syntax
    pub fn java() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxJava) }
    }

    /// Perl syntax
    pub fn perl() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxPerl) }
    }

    /// Perl + named group syntax
    pub fn perl_ng() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxPerl_NG) }
    }

    /// Ruby syntax
    pub fn ruby() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxRuby) }
    }

    /// Oniguruma Syntax
    pub fn oniguruma() -> &'static Syntax {
        unsafe { transmute(&onig_sys::OnigSyntaxOniguruma) }
    }

    /// Default syntax (Ruby syntax)
    pub fn default() -> &'static Syntax {
        unsafe { transmute(onig_sys::OnigDefaultSyntax) }
    }

    /// Retrieve the operators for this syntax
    pub fn operators(&self) -> SyntaxOperator {
        SyntaxOperator::from_bits_truncate(self.operators_bits())
    }

    /// Retrieve the raw operator bits
    fn operators_bits(&self) -> u64 {
        unsafe {
            let op = onig_sys::onig_get_syntax_op(self.raw_mut());
            let op2 = onig_sys::onig_get_syntax_op2(self.raw_mut());
            u64::from(op) + (u64::from(op2) << 32)
        }
    }

    /// Replace the operators for this syntax
    pub fn set_operators(&mut self, operators: SyntaxOperator) {
        self.set_operators_bits(operators.bits())
    }

    /// Replace the operators for this syntax with the given raw bits
    fn set_operators_bits(&mut self, operators_bits: u64) {
        let op = operators_bits as onig_sys::OnigSyntaxOp;
        let op2 = (operators_bits >> 32) as onig_sys::OnigSyntaxOp2;
        unsafe {
            onig_sys::onig_set_syntax_op(&mut self.raw, op);
            onig_sys::onig_set_syntax_op2(&mut self.raw, op2)
        }
    }

    /// Enable Operators for this Syntax
    ///
    /// Updates the operators for this syntax to enable the chosen
    /// ones.
    pub fn enable_operators(&mut self, operators: SyntaxOperator) {
        let operators = self.operators_bits() | operators.bits();
        self.set_operators_bits(operators)
    }

    /// Disable Operators for this Syntax
    ///
    /// Updates the operators for this syntax to remove the specified
    /// operators.
    pub fn disable_operators(&mut self, operators: SyntaxOperator) {
        let operators = self.operators_bits() & !operators.bits();
        self.set_operators_bits(operators)
    }

    /// Retrieves the syntax behaviours
    pub fn behavior(&self) -> SyntaxBehavior {
        SyntaxBehavior::from_bits_truncate(unsafe {
            onig_sys::onig_get_syntax_behavior(self.raw_mut())
        })
    }

    /// Overwrite the syntax behaviour for this syntax.
    pub fn set_behavior(&mut self, behavior: SyntaxBehavior) {
        let behavior = behavior.bits() as onig_sys::OnigSyntaxBehavior;
        unsafe {
            onig_sys::onig_set_syntax_behavior(&mut self.raw, behavior);
        }
    }

    /// Enable a given behaviour for this syntax
    pub fn enable_behavior(&mut self, behavior: SyntaxBehavior) {
        let behavior = self.behavior() | behavior;
        self.set_behavior(behavior)
    }

    /// Disable a given behaviour for this syntax
    pub fn disable_behavior(&mut self, behavior: SyntaxBehavior) {
        let behavior = self.behavior() & !behavior;
        self.set_behavior(behavior)
    }

    /// Retireve the syntax options for this syntax
    pub fn options(&self) -> RegexOptions {
        RegexOptions::from_bits_truncate(unsafe {
            onig_sys::onig_get_syntax_options(self.raw_mut())
        })
    }

    /// Replace the syntax options for this syntax
    pub fn set_options(&mut self, options: RegexOptions) {
        let options = options.bits() as onig_sys::OnigOptionType;
        unsafe {
            onig_sys::onig_set_syntax_options(&mut self.raw, options);
        }
    }

    /// Set a given meta character's state
    ///
    /// Arguments:
    ///  - `what`: The meta character to update
    ///  - `meta`: The value to set the meta character to
    pub fn set_meta_char(&mut self, what: MetaCharType, meta: MetaChar) {
        let what = what.bits();
        let code = match meta {
            MetaChar::Ineffective => onig_sys::ONIG_INEFFECTIVE_META_CHAR,
            MetaChar::Character(char) => char as u32,
        };
        unsafe {
            onig_sys::onig_set_meta_char(&mut self.raw, what, code);
        }
    }

    fn raw_mut(&self) -> *mut onig_sys::OnigSyntaxType {
        &self.raw as *const onig_sys::OnigSyntaxType as *mut onig_sys::OnigSyntaxType
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn round_trip_bits() {
        let mut syn = Syntax::python().clone();
        syn.enable_operators(SyntaxOperator::SYNTAX_OPERATOR_ESC_X_BRACE_HEX8);
        assert_ne!(Syntax::python().raw, syn.raw);
        syn.disable_operators(SyntaxOperator::SYNTAX_OPERATOR_ESC_X_BRACE_HEX8);
        assert_eq!(Syntax::python().raw, syn.raw);
    }
}
