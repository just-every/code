use bitflags::bitflags;
use std::os::raw::c_uint;

bitflags! {
    /// Regex parsing and compilation options.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct RegexOptions: onig_sys::OnigOptionType {
        /// Default options.
        const REGEX_OPTION_NONE
            = onig_sys::ONIG_OPTION_NONE;
        /// Ambiguity match on.
        const REGEX_OPTION_IGNORECASE
            = onig_sys::ONIG_OPTION_IGNORECASE;
        /// Extended pattern form.
        const REGEX_OPTION_EXTEND
            = onig_sys::ONIG_OPTION_EXTEND;
        /// `'.'` match with newline.
        const REGEX_OPTION_MULTILINE
            = onig_sys::ONIG_OPTION_MULTILINE;
        /// `'^'` -> `'\A'`, `'$'` -> `'\Z'`.
        const REGEX_OPTION_SINGLELINE
            = onig_sys::ONIG_OPTION_SINGLELINE;
        /// Find longest match.
        const REGEX_OPTION_FIND_LONGEST
            = onig_sys::ONIG_OPTION_FIND_LONGEST;
        /// Ignore empty match.
        const REGEX_OPTION_FIND_NOT_EMPTY
            = onig_sys::ONIG_OPTION_FIND_NOT_EMPTY;
        /// Clear `OPTION_SINGLELINE` which is enabled on
        /// `SYNTAX_POSIX_BASIC`, `SYNTAX_POSIX_EXTENDED`,
        /// `SYNTAX_PERL`, `SYNTAX_PERL_NG`, `SYNTAX_JAVA`.
        const REGEX_OPTION_NEGATE_SINGLELINE
            = onig_sys::ONIG_OPTION_NEGATE_SINGLELINE;
        /// Only named group captured.
        const REGEX_OPTION_DONT_CAPTURE_GROUP
            = onig_sys::ONIG_OPTION_DONT_CAPTURE_GROUP;
        /// Named and no-named group captured.
        const REGEX_OPTION_CAPTURE_GROUP
            = onig_sys::ONIG_OPTION_CAPTURE_GROUP;
    }
}

bitflags! {
    /// Regex evaluation options.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct SearchOptions: onig_sys::OnigOptionType {
        /// Default options.
        const SEARCH_OPTION_NONE
            = onig_sys::ONIG_OPTION_NONE;
        /// String head isn't considered as begin of line.
        const SEARCH_OPTION_NOTBOL
            = onig_sys::ONIG_OPTION_NOTBOL;
        /// String end isn't considered as end of line.
        const SEARCH_OPTION_NOTEOL
            = onig_sys::ONIG_OPTION_NOTEOL;
        /// Try and match the pattern against the whole string.
        const SEARCH_OPTION_WHOLE_STRING
            = onig_sys::ONIG_OPTION_MATCH_WHOLE_STRING;
    }
}

bitflags! {
    /// Defines the different operators allowed within a regex syntax.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct SyntaxOperator: u64 {
        /// `.`
        const SYNTAX_OPERATOR_DOT_ANYCHAR
            = (onig_sys::ONIG_SYN_OP_DOT_ANYCHAR as u64);
        /// `*`
        const SYNTAX_OPERATOR_ASTERISK_ZERO_INF
            = (onig_sys::ONIG_SYN_OP_ASTERISK_ZERO_INF as u64);
        /// `+`
        const SYNTAX_OPERATOR_PLUS_ONE_INF
            = (onig_sys::ONIG_SYN_OP_PLUS_ONE_INF as u64);
        /// `?`
        const SYNTAX_OPERATOR_QMARK_ZERO_ONE
            = (onig_sys::ONIG_SYN_OP_QMARK_ZERO_ONE as u64);
    /// ?P
    const SYNTAX_OPERATOR_QMARK_CAPITAL_P_NAME
        = (onig_sys::ONIG_SYN_OP2_QMARK_CAPITAL_P_NAME as u64);
        /// `{lower,upper}`
        const SYNTAX_OPERATOR_BRACE_INTERVAL
            = (onig_sys::ONIG_SYN_OP_BRACE_INTERVAL as u64);
        /// `\{lower,upper\}`
        const SYNTAX_OPERATOR_ESC_BRACE_INTERVAL
            = (onig_sys::ONIG_SYN_OP_ESC_BRACE_INTERVAL as u64);
        /// `|`
        const SYNTAX_OPERATOR_VBAR_ALT
            = (onig_sys::ONIG_SYN_OP_VBAR_ALT as u64);
        /// `\|`
        const SYNTAX_OPERATOR_ESC_VBAR_ALT
            = (onig_sys::ONIG_SYN_OP_ESC_VBAR_ALT as u64);
        /// `(...)`
        const SYNTAX_OPERATOR_LPAREN_SUBEXP
            = (onig_sys::ONIG_SYN_OP_LPAREN_SUBEXP as u64);
        /// `\(...\)`
        const SYNTAX_OPERATOR_ESC_LPAREN_SUBEXP
            = (onig_sys::ONIG_SYN_OP_ESC_LPAREN_SUBEXP as u64);
        /// `\A, \Z, \z`
        const SYNTAX_OPERATOR_ESC_AZ_BUF_ANCHOR
            = (onig_sys::ONIG_SYN_OP_ESC_AZ_BUF_ANCHOR as u64);
        /// `\G`
        const SYNTAX_OPERATOR_ESC_CAPITAL_G_BEGIN_ANCHOR
            = (onig_sys::ONIG_SYN_OP_ESC_CAPITAL_G_BEGIN_ANCHOR as u64);
        /// `\num`
        const SYNTAX_OPERATOR_DECIMAL_BACKREF
            = (onig_sys::ONIG_SYN_OP_DECIMAL_BACKREF as u64);
        /// `[...]`
        const SYNTAX_OPERATOR_BRACKET_CC
            = (onig_sys::ONIG_SYN_OP_BRACKET_CC as u64);
        /// `\w, \W`
        const SYNTAX_OPERATOR_ESC_W_WORD
            = (onig_sys::ONIG_SYN_OP_ESC_W_WORD as u64);
        /// `\<. \>`
        const SYNTAX_OPERATOR_ESC_LTGT_WORD_BEGIN_END
            = (onig_sys::ONIG_SYN_OP_ESC_LTGT_WORD_BEGIN_END as u64);
        /// `\b, \B`
        const SYNTAX_OPERATOR_ESC_B_WORD_BOUND
            = (onig_sys::ONIG_SYN_OP_ESC_B_WORD_BOUND as u64);
        /// `\s, \S`
        const SYNTAX_OPERATOR_ESC_S_WHITE_SPACE
            = (onig_sys::ONIG_SYN_OP_ESC_S_WHITE_SPACE as u64);
        /// `\d, \D`
        const SYNTAX_OPERATOR_ESC_D_DIGIT
            = (onig_sys::ONIG_SYN_OP_ESC_D_DIGIT as u64);
        /// `^, $`
        const SYNTAX_OPERATOR_LINE_ANCHOR
            = (onig_sys::ONIG_SYN_OP_LINE_ANCHOR as u64);
        /// `[:xxxx:]`
        const SYNTAX_OPERATOR_POSIX_BRACKET
            = (onig_sys::ONIG_SYN_OP_POSIX_BRACKET as u64);
        /// `??,*?,+?,{n,m}?`
        const SYNTAX_OPERATOR_QMARK_NON_GREEDY
            = (onig_sys::ONIG_SYN_OP_QMARK_NON_GREEDY as u64);
        /// `\n,\r,\t,\a ...`
        const SYNTAX_OPERATOR_ESC_CONTROL_CHARS
            = (onig_sys::ONIG_SYN_OP_ESC_CONTROL_CHARS as u64);
        /// `\cx`
        const SYNTAX_OPERATOR_ESC_C_CONTROL
            = (onig_sys::ONIG_SYN_OP_ESC_C_CONTROL as u64);
        /// `\OOO`
        const SYNTAX_OPERATOR_ESC_OCTAL3
            = (onig_sys::ONIG_SYN_OP_ESC_OCTAL3 as u64);
        /// `\xHH`
        const SYNTAX_OPERATOR_ESC_X_HEX2
            = (onig_sys::ONIG_SYN_OP_ESC_X_HEX2 as u64);
        /// `\x{7HHHHHHH}`
        const SYNTAX_OPERATOR_ESC_X_BRACE_HEX8
            = (onig_sys::ONIG_SYN_OP_ESC_X_BRACE_HEX8 as u64);
        /// Variable meta characters
        const SYNTAX_OPERATOR_VARIABLE_META_CHARACTERS
            = (onig_sys::ONIG_SYN_OP_VARIABLE_META_CHARACTERS as u64);
        /// `\Q...\E`
        const SYNTAX_OPERATOR_ESC_CAPITAL_Q_QUOTE
            = (onig_sys::ONIG_SYN_OP2_ESC_CAPITAL_Q_QUOTE as u64) << 32;
        /// `(?...)`
        const SYNTAX_OPERATOR_QMARK_GROUP_EFFECT
            = (onig_sys::ONIG_SYN_OP2_QMARK_GROUP_EFFECT as u64) << 32;
        /// `(?imsx),(?-imsx)`
        const SYNTAX_OPERATOR_OPTION_PERL
            = (onig_sys::ONIG_SYN_OP2_OPTION_PERL as u64) << 32;
        /// `(?imx), (?-imx)`
        const SYNTAX_OPERATOR_OPTION_RUBY
            = (onig_sys::ONIG_SYN_OP2_OPTION_RUBY as u64) << 32;
        /// `?+,*+,++`
        const SYNTAX_OPERATOR_PLUS_POSSESSIVE_REPEAT
            = (onig_sys::ONIG_SYN_OP2_PLUS_POSSESSIVE_REPEAT as u64) << 32;
        /// `{n,m}+`
        const SYNTAX_OPERATOR_PLUS_POSSESSIVE_INTERVAL
            = (onig_sys::ONIG_SYN_OP2_PLUS_POSSESSIVE_INTERVAL as u64) << 32;
        /// `[...&&..[..]..]`
        const SYNTAX_OPERATOR_CCLASS_SET_OP
            = (onig_sys::ONIG_SYN_OP2_CCLASS_SET_OP as u64) << 32;
        /// `(?<name>...)`
        const SYNTAX_OPERATOR_QMARK_LT_NAMED_GROUP
            = (onig_sys::ONIG_SYN_OP2_QMARK_LT_NAMED_GROUP as u64) << 32;
        /// `\k<name>`
        const SYNTAX_OPERATOR_ESC_K_NAMED_BACKREF
            = (onig_sys::ONIG_SYN_OP2_ESC_K_NAMED_BACKREF as u64) << 32;
        /// `\g<name>, \g<n>`
        const SYNTAX_OPERATOR_ESC_G_SUBEXP_CALL
            = (onig_sys::ONIG_SYN_OP2_ESC_G_SUBEXP_CALL as u64) << 32;
        /// `(?@..),(?@<x>..)`
        const SYNTAX_OPERATOR_ATMARK_CAPTURE_HISTORY
            = (onig_sys::ONIG_SYN_OP2_ATMARK_CAPTURE_HISTORY as u64) << 32;
        /// `\C-x`
        const SYNTAX_OPERATOR_ESC_CAPITAL_C_BAR_CONTROL
            = (onig_sys::ONIG_SYN_OP2_ESC_CAPITAL_C_BAR_CONTROL as u64) << 32;
        /// `\M-x`
        const SYNTAX_OPERATOR_ESC_CAPITAL_M_BAR_META
            = (onig_sys::ONIG_SYN_OP2_ESC_CAPITAL_M_BAR_META as u64) << 32;
        /// `\v as VTAB`
        const SYNTAX_OPERATOR_ESC_V_VTAB
            = (onig_sys::ONIG_SYN_OP2_ESC_V_VTAB as u64) << 32;
        /// `\uHHHH`
        const SYNTAX_OPERATOR_ESC_U_HEX4
            = (onig_sys::ONIG_SYN_OP2_ESC_U_HEX4 as u64) << 32;
        /// `\`, \'`
        const SYNTAX_OPERATOR_ESC_GNU_BUF_ANCHOR
            = (onig_sys::ONIG_SYN_OP2_ESC_GNU_BUF_ANCHOR as u64) << 32;
        /// `\p{...}, \P{...}`
        const SYNTAX_OPERATOR_ESC_P_BRACE_CHAR_PROPERTY
            = (onig_sys::ONIG_SYN_OP2_ESC_P_BRACE_CHAR_PROPERTY as u64) << 32;
        /// `\p{^..}, \P{^..}`
        const SYNTAX_OPERATOR_ESC_P_BRACE_CIRCUMFLEX_NOT
            = (onig_sys::ONIG_SYN_OP2_ESC_P_BRACE_CIRCUMFLEX_NOT as u64) << 32;
        /// `\h, \H`
        const SYNTAX_OPERATOR_ESC_H_XDIGIT
            = (onig_sys::ONIG_SYN_OP2_ESC_H_XDIGIT as u64) << 32;
        /// `\`
        const SYNTAX_OPERATOR_INEFFECTIVE_ESCAPE
            = (onig_sys::ONIG_SYN_OP2_INEFFECTIVE_ESCAPE as u64) << 32;
    }
}

bitflags! {
    /// Defines the behaviour of regex operators.
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct SyntaxBehavior: onig_sys::OnigSyntaxBehavior {
        /// `?, *, +, {n,m}`
        const SYNTAX_BEHAVIOR_CONTEXT_INDEP_REPEAT_OPS
            = onig_sys::ONIG_SYN_CONTEXT_INDEP_REPEAT_OPS;
        /// `error or ignore`
        const SYNTAX_BEHAVIOR_CONTEXT_INVALID_REPEAT_OPS
            = onig_sys::ONIG_SYN_CONTEXT_INVALID_REPEAT_OPS;
        /// `...)...`
        const SYNTAX_BEHAVIOR_ALLOW_UNMATCHED_CLOSE_SUBEXP
            = onig_sys::ONIG_SYN_ALLOW_UNMATCHED_CLOSE_SUBEXP;
        /// `{???`
        const SYNTAX_BEHAVIOR_ALLOW_INVALID_INTERVAL
            = onig_sys::ONIG_SYN_ALLOW_INVALID_INTERVAL;
        /// `{,n} => {0,n}`
        const SYNTAX_BEHAVIOR_ALLOW_INTERVAL_LOW_ABBREV
            = onig_sys::ONIG_SYN_ALLOW_INTERVAL_LOW_ABBREV;
        /// `/(\1)/,/\1()/ ..`
        const SYNTAX_BEHAVIOR_STRICT_CHECK_BACKREF
            = onig_sys::ONIG_SYN_STRICT_CHECK_BACKREF;
        /// `(?<=a|bc)`
        const SYNTAX_BEHAVIOR_DIFFERENT_LEN_ALT_LOOK_BEHIND
            = onig_sys::ONIG_SYN_DIFFERENT_LEN_ALT_LOOK_BEHIND;
        /// See Oniguruma documenation
        const SYNTAX_BEHAVIOR_CAPTURE_ONLY_NAMED_GROUP
            = onig_sys::ONIG_SYN_CAPTURE_ONLY_NAMED_GROUP;
        /// `(?<x>)(?<x>)`
        const SYNTAX_BEHAVIOR_ALLOW_MULTIPLEX_DEFINITION_NAME
            = onig_sys::ONIG_SYN_ALLOW_MULTIPLEX_DEFINITION_NAME;
        /// `a{n}?=(?:a{n})?`
        const SYNTAX_BEHAVIOR_FIXED_INTERVAL_IS_GREEDY_ONLY
            = onig_sys::ONIG_SYN_FIXED_INTERVAL_IS_GREEDY_ONLY;
        /// `[^...]`
        const SYNTAX_BEHAVIOR_NOT_NEWLINE_IN_NEGATIVE_CC
            = onig_sys::ONIG_SYN_NOT_NEWLINE_IN_NEGATIVE_CC;
        /// `[..\w..] etc..`
        const SYNTAX_BEHAVIOR_BACKSLASH_ESCAPE_IN_CC
            = onig_sys::ONIG_SYN_BACKSLASH_ESCAPE_IN_CC;
        /// `[0-9-a]=[0-9\-a]`
        const SYNTAX_BEHAVIOR_ALLOW_DOUBLE_RANGE_OP_IN_CC
            = onig_sys::ONIG_SYN_ALLOW_DOUBLE_RANGE_OP_IN_CC;
        /// `[,-,]`
        const SYNTAX_BEHAVIOR_WARN_CC_OP_NOT_ESCAPED
            = onig_sys::ONIG_SYN_WARN_CC_OP_NOT_ESCAPED;
        /// `(?:a*)+`
        const SYNTAX_BEHAVIOR_WARN_REDUNDANT_NESTED_REPEAT
            = onig_sys::ONIG_SYN_WARN_REDUNDANT_NESTED_REPEAT;
    }
}

bitflags! {
    /// The order in which traverse callbacks are invoked
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct TraverseCallbackAt: c_uint {
        /// Callback before children are wallked
        const CALLBACK_AT_FIRST =
            onig_sys::ONIG_TRAVERSE_CALLBACK_AT_FIRST;
        /// Callback after children are walked
        const CALLBACK_AT_LAST =
            onig_sys::ONIG_TRAVERSE_CALLBACK_AT_LAST;
        /// Callback both before and after children are walked.
        const CALLBACK_AT_BOTH =
            onig_sys::ONIG_TRAVERSE_CALLBACK_AT_BOTH;
    }
}

bitflags! {
    /// Syntax meta character types
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct MetaCharType: c_uint {
        /// The escape charater for this syntax
        const META_CHAR_ESCAPE = onig_sys::ONIG_META_CHAR_ESCAPE;
        /// The any (.) character for this syntax.
        const META_CHAR_ANYCHAR =  onig_sys::ONIG_META_CHAR_ANYCHAR;
        /// The any number of repeats (*) character for this syntax.
        const META_CHAR_ANYTIME =  onig_sys::ONIG_META_CHAR_ANYTIME;
        /// The optinoal (?) chracter for this syntax
        const META_CHAR_ZERO_OR_ONE_TIME =  onig_sys::ONIG_META_CHAR_ZERO_OR_ONE_TIME;
        /// The at least once (+) character for this syntax
        const META_CHAR_ONE_OR_MORE_TIME =  onig_sys::ONIG_META_CHAR_ONE_OR_MORE_TIME;
        /// The glob character for this syntax (.*)
        const META_CHAR_ANYCHAR_ANYTIME =  onig_sys::ONIG_META_CHAR_ANYCHAR_ANYTIME;
    }
}
