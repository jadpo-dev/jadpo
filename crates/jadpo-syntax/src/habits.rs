//! Habits from other languages (area 3, "Habits from other languages"): constructs people and agents write out of
//! muscle memory from TypeScript and JavaScript, Python, Rust, Go, Java and
//! C#, Kotlin and Swift, and SQL, each with an unambiguous Jadpo spelling.
//!
//! [`HABITS`] is the catalogue: every entry names its rule, where it is
//! found (the lexer, the parser at one position, or the checker for a name
//! that resolves to nothing), what is written, the Jadpo form, and the
//! words of its error. The lexer, the parser and the checker build their
//! errors from these entries, the corpus test checks each one is reported
//! once on its line, and each rule has a checked example on the page of
//! the area that decides its Jadpo form.
//!
//! In the texts, `$W` stands for what was written and `$J` for the Jadpo
//! form, both as found in the source when the error is made.
//!
//! Left out, because the Jadpo spelling depends on something the source
//! doesn't say: `this` and `self` (Jadpo has no receiver; which parameter
//! is meant isn't written), `print` and `console.log` (logging isn't a
//! callable yet), `number`, `float` and `double` (`Int` or a `Decimal`
//! with a scale), `Date`, `DateTime` and `timestamp` (`Instant` or
//! `CalendarDate`), `any`, `object` and `unknown`, `class` (an `Object`, an
//! entity or a domain), `while`, `loop` and `do` (no loop but `for` over a
//! list; `syntax.expected_statement` says so), `break` and `continue`
//! outside a `switch`, string interpolation (joining needs both sides'
//! types), Go's `:=` (the type isn't written) and `--` comments (`--` is
//! also two minus signs).

use jadpo_diag::{Rule, rule};

/// Where a habit is found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detected {
    /// While reading characters into tokens.
    Lexer,
    /// While parsing, at the position the entry's error is made.
    Parser,
    /// While checking: a name that resolves to nothing.
    Checker,
}

/// One construct carried over from another language.
#[derive(Debug, PartialEq, Eq)]
pub struct Habit {
    pub rule: &'static Rule,
    pub detected: Detected,
    /// What is written, with `T` for a type and `x` for a name.
    pub written: &'static str,
    /// The Jadpo form, with the same placeholders.
    pub jadpo: &'static str,
    pub headline: &'static str,
    pub label: &'static str,
    /// Names the habit's languages, then says what Jadpo does instead.
    pub reason: &'static str,
    pub next: &'static str,
}

impl Habit {
    fn fill(text: &str, written: &str, jadpo: &str) -> String {
        text.replace("$W", written).replace("$J", jadpo)
    }

    pub fn message(&self, written: &str, jadpo: &str) -> String {
        Habit::fill(self.headline, written, jadpo)
    }

    pub fn label_for(&self, written: &str, jadpo: &str) -> String {
        Habit::fill(self.label, written, jadpo)
    }

    pub fn next_for(&self, written: &str, jadpo: &str) -> String {
        Habit::fill(self.next, written, jadpo)
    }
}

macro_rules! habits {
    ($($name:ident: $rule:ident, $detected:ident, $written:literal -> $jadpo:literal,
        $headline:literal, $label:literal, $reason:literal, $next:literal;)*) => {
        $(
            #[doc = $reason]
            pub static $name: Habit = Habit {
                rule: &rule::$rule,
                detected: Detected::$detected,
                written: $written,
                jadpo: $jadpo,
                headline: $headline,
                label: $label,
                reason: $reason,
                next: $next,
            };
        )*

        /// Every habit, in the order of the decision's survey.
        pub static HABITS: &[&Habit] = &[$(&$name),*];
    };
}

habits! {
    // --- comparisons and logic (area 3) --------------------------------------
    STRICT_EQUAL: SYNTAX_EQUALITY_OPERATOR, Lexer, "===" -> "==",
        "Jadpo compares with `$J`, not `$W`", "write `$J`",
        "`===` and `!==` compare without converting types in JavaScript and TypeScript; Jadpo's `==` and `!=` never convert between types, so there is no stricter form to ask for.",
        "Use `$J` instead of `$W`";
    STRICT_NOT_EQUAL: SYNTAX_EQUALITY_OPERATOR, Lexer, "!==" -> "!=",
        "Jadpo compares with `$J`, not `$W`", "write `$J`",
        "`===` and `!==` compare without converting types in JavaScript and TypeScript; Jadpo's `==` and `!=` never convert between types, so there is no stricter form to ask for.",
        "Use `$J` instead of `$W`";
    SQL_NOT_EQUAL: SYNTAX_EQUALITY_OPERATOR, Lexer, "<>" -> "!=",
        "Jadpo compares with `$J`, not `$W`", "write `$J`",
        "`<>` means not equal in SQL; Jadpo writes `!=`.",
        "Use `$J` instead of `$W`";
    SINGLE_EQUAL: SYNTAX_EQUALITY_OPERATOR, Parser, "=" -> "==",
        "Jadpo compares with `$J`, not `$W`", "write `$J`",
        "`=` compares in SQL; in Jadpo `=` gives a variable its value, and `==` compares.",
        "Use `$J` instead of `$W`";
    SQL_IS_NULL: SYNTAX_EQUALITY_OPERATOR, Parser, "IS NULL" -> "== none",
        "Jadpo compares with `$J`, not `$W`", "write `$J`",
        "`IS NULL` and `IS NOT NULL` test for a missing value in SQL; Jadpo compares with `none` as with any other value.",
        "Use `$J` instead of `$W`";
    PYTHON_IS_NONE: SYNTAX_EQUALITY_OPERATOR, Parser, "is None" -> "== none",
        "Jadpo compares with `$J`, not `$W`", "write `$J`",
        "`is None` and `is not None` test for a missing value in Python; Jadpo compares with `none` as with any other value.",
        "Use `$J` instead of `$W`";
    SYMBOL_AND: SYNTAX_LOGICAL_OPERATOR, Lexer, "&&" -> "and",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`&&`, `||` and `!` are the logical operators of JavaScript, Rust, Go, Java, C#, Kotlin and Swift; Jadpo writes them as words: `and`, `or` and `not`.",
        "Use `$J` instead of `$W`";
    SYMBOL_OR: SYNTAX_LOGICAL_OPERATOR, Lexer, "||" -> "or",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`&&`, `||` and `!` are the logical operators of JavaScript, Rust, Go, Java, C#, Kotlin and Swift; Jadpo writes them as words: `and`, `or` and `not`.",
        "Use `$J` instead of `$W`";
    SYMBOL_NOT: SYNTAX_LOGICAL_OPERATOR, Lexer, "!" -> "not",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`&&`, `||` and `!` are the logical operators of JavaScript, Rust, Go, Java, C#, Kotlin and Swift; Jadpo writes them as words: `and`, `or` and `not`.",
        "Use `$J` instead of `$W`";
    CAPITAL_LOGIC: SYNTAX_LOGICAL_OPERATOR, Parser, "AND" -> "and",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "SQL writes `AND`, `OR` and `NOT` in capitals; Jadpo's logical operators are the lower-case words `and`, `or` and `not`.",
        "Use `$J` instead of `$W`";
    TERNARY: SYNTAX_TERNARY_OPERATOR, Parser, "c ? a : b" -> "match",
        "Jadpo chooses a value with `match`, not `? :`", "write a `match`",
        "`condition ? a : b` chooses a value in JavaScript, Java, C#, Kotlin and Swift; Jadpo chooses between two values with a `match` on the condition.",
        "Write $J";
    INCREMENT: SYNTAX_INCREMENT_OPERATOR, Parser, "x++" -> "x = x + 1",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`++`, `--`, `+=`, `-=` and `*=` change a variable in place in JavaScript, Java, C#, Go, Kotlin, Swift and Python; Jadpo assigns a `var mut` with `=`, and a named type converts the result back with its constructor, as in `count = Count(count + 1)`.",
        "Write `$J`";

    // --- statements and declarations (area 3) --------------------------------
    SEMICOLON: SYNTAX_SEMICOLON, Lexer, ";" -> "",
        "Jadpo has no `;`", "remove `;`",
        "`;` ends a statement in JavaScript, Rust, Go, Java, C#, Kotlin, Swift and SQL; a Jadpo statement or member ends at the end of its line.",
        "Remove `;`";
    LET: SYNTAX_VARIABLE_KEYWORD, Parser, "let" -> "var",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`let` declares a variable in JavaScript, TypeScript, Swift and Rust; a Jadpo variable is `var name: Type = value`, or `var mut` when it can change.",
        "Use `$J` instead of `$W`";
    LET_MUT: SYNTAX_VARIABLE_KEYWORD, Parser, "let mut" -> "var mut",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`let mut` declares a variable that can change in Rust; Jadpo writes `var mut name: Type = value`.",
        "Use `$J` instead of `$W`";
    CONST: SYNTAX_VARIABLE_KEYWORD, Parser, "const" -> "var",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`const` declares a variable that never changes in JavaScript, TypeScript and C#; a Jadpo `var` never changes unless it is `var mut`.",
        "Use `$J` instead of `$W`";
    CONST_DECL: SYNTAX_VARIABLE_KEYWORD, Parser, "const" -> "constant",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`const` declares a value that never changes in JavaScript, TypeScript, Rust, Go and C#; at the top of a Jadpo file a fixed value is `constant name: Type = value`.",
        "Use `$J` instead of `$W`";
    VAL: SYNTAX_VARIABLE_KEYWORD, Parser, "val" -> "var",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`val` declares a variable that never changes in Kotlin and Scala; a Jadpo `var` never changes unless it is `var mut`.",
        "Use `$J` instead of `$W`";
    FN: SYNTAX_FUNCTION_KEYWORD, Parser, "fn" -> "function",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`fn` declares a function in Rust; Jadpo starts one with `function`, or `action` when it reads or writes stored data.",
        "Use `$J` instead of `$W`";
    FUNC: SYNTAX_FUNCTION_KEYWORD, Parser, "func" -> "function",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`func` declares a function in Go and Swift; Jadpo starts one with `function`, or `action` when it reads or writes stored data.",
        "Use `$J` instead of `$W`";
    DEF: SYNTAX_FUNCTION_KEYWORD, Parser, "def" -> "function",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`def` declares a function in Python and Ruby; Jadpo starts one with `function`, or `action` when it reads or writes stored data.",
        "Use `$J` instead of `$W`";
    FUN: SYNTAX_FUNCTION_KEYWORD, Parser, "fun" -> "function",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`fun` declares a function in Kotlin; Jadpo starts one with `function`, or `action` when it reads or writes stored data.",
        "Use `$J` instead of `$W`";
    COLON_RESULT: SYNTAX_RETURN_TYPE_ARROW, Parser, ": T" -> "-> T",
        "Jadpo writes the result type after `->`, not `:`", "write `->`",
        "`: Type` after the parameters is the result type in TypeScript and Kotlin; Jadpo writes `-> Type`, as in `function total(order: Order) -> Amount {`.",
        "Use `->` instead of `:`";
    ASYNC: SYNTAX_ASYNC_AWAIT, Parser, "async" -> "",
        "Jadpo has no `async`", "remove `async`",
        "`async` and `await` mark code that waits for a result in JavaScript, TypeScript, Python, Rust, C# and Swift; a Jadpo call always gives its result when it returns, so there is nothing to mark.",
        "Remove `async`";
    AWAIT: SYNTAX_ASYNC_AWAIT, Parser, "await" -> "",
        "Jadpo has no `await`", "remove `await`",
        "`async` and `await` mark code that waits for a result in JavaScript, TypeScript, Python, Rust, C# and Swift; a Jadpo call always gives its result when it returns, so there is nothing to mark.",
        "Remove `await`";
    ELIF: SYNTAX_ELSE_IF, Parser, "elif" -> "else if",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`elif` continues an `if` in Python, `elsif` in Ruby and `elseif` in PHP and Lua; Jadpo writes `else if`.",
        "Use `$J` instead of `$W`";
    FOR_OF: SYNTAX_FOR_LOOP, Parser, "for (const x of xs)" -> "for x in xs",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`for (const item of items)` walks a list in JavaScript and TypeScript, `for (item in items)` in Kotlin, `for (Type item : items)` in Java and `foreach (var item in items)` in C#; Jadpo writes `for item in items {`, without brackets.",
        "Write `$J`";
    FOR_COUNTING: SYNTAX_FOR_LOOP, Parser, "for (init; condition; step)" -> "for x in xs",
        "Jadpo has no counting `for` loop", "not a Jadpo loop",
        "`for (i = 0; i < n; i++)` counts in C-family languages; Jadpo's only loop is `for item in items`, over a list, which always ends.",
        "Loop over the list itself with `for item in items`";
    SWITCH: SYNTAX_SWITCH_STATEMENT, Parser, "switch" -> "match",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`switch` chooses between cases in JavaScript, Java, C#, Go and Swift, and `when` in Kotlin; Jadpo's `match` does, with one arm per line, `pattern -> result`, covering every case.",
        "Use `$J` instead of `$W`";
    CASE: SYNTAX_SWITCH_STATEMENT, Parser, "case P:" -> "P ->",
        "A Jadpo `match` arm has no `case`", "remove `case`",
        "`case` starts an arm of a `switch` in JavaScript, Java, C#, Go and Swift; a Jadpo `match` arm is `pattern -> result`, and ends at its result, with no `break`.",
        "Write the arm as `$J`";
    DEFAULT: SYNTAX_SWITCH_STATEMENT, Parser, "default:" -> "_ ->",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`default` is a `switch`'s catch-all in JavaScript, Java, C#, Go and Swift, and `else` a `when`'s in Kotlin; Jadpo's is `_`, used only where the cases can't be listed.",
        "Write `$J`";
    TRY_CALL: SYNTAX_TRY_CATCH, Parser, "try" -> "attempt",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`try` marks a call that can fail in Swift; Jadpo writes `attempt`, which passes the call's failures on to the caller.",
        "Use `$J` instead of `$W`";
    TRY_BLOCK: SYNTAX_TRY_CATCH, Parser, "try {" -> "attempt",
        "Jadpo has no `$W` block", "not Jadpo",
        "`try`, `catch` and `finally` wrap code that can fail in JavaScript, Java, C#, Kotlin and Python; Jadpo marks each call that can fail where it is made: `attempt` passes its failures on, and a `match` on the call handles each one.",
        "Mark each call that can fail with `attempt`, or handle its failures with `match call(...) { ... }`";
    QUESTION_MARK: SYNTAX_TRY_CATCH, Parser, "x?" -> "attempt x",
        "Jadpo writes `attempt`, not `?`", "write `attempt` before the call",
        "`?` after a call passes its failure on in Rust; Jadpo writes `attempt` before the call.",
        "Write `$J`";
    THROW: SYNTAX_UNSUPPORTED_THROW, Parser, "throw" -> "reject",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`throw` raises an error in JavaScript, Java, C#, Kotlin and Swift, and `raise` in Python and Ruby; a Jadpo callable raises one of its declared failures with `reject`, and lists it in `fails`.",
        "Use `$J` instead of `$W`";
    FAT_ARROW_ARM: SYNTAX_MATCH_ARROW, Parser, "=>" -> "->",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`=>` ends a match arm's pattern in Rust, Scala, C# and PHP; a Jadpo arm uses `->`, the same arrow as a result type.",
        "Use `$J` instead of `$W`";
    ARROW_FUNCTION: SYNTAX_INLINE_FUNCTION_ARROW, Parser, "x => e" -> "x -> e",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`x => e` is an arrow function in JavaScript, TypeScript and C#, `lambda x: e` in Python and `|x| e` in Rust; a Jadpo inline function is `item -> expression`.",
        "Write `$J`";
    NAMED_EQUALS: SYNTAX_NAMED_ARGUMENT_EQUALS, Parser, "name = value" -> "name: value",
        "Jadpo names an argument with `:`, not `=`", "write `:`",
        "`name = value` names an argument in Python, Kotlin and C#; Jadpo writes `name: value`.",
        "Use `:` instead of `=`";
    MEMBER_EQUALS: SYNTAX_COLON_REQUIRED, Parser, "name = value" -> "name: value",
        "Jadpo writes `$J`, not `$W`", "write `:`",
        "`name = value` sets a field in C# and Kotlin and a case's value in TypeScript; every member inside braces is `name: value`.",
        "Use `:` instead of `=`";
    MEMBER_COMMA: SYNTAX_MEMBER_COMMA, Parser, "," -> "",
        "Jadpo has no `,` between members", "remove `,`",
        "Commas separate fields in JavaScript, TypeScript, Rust, Go, Swift and JSON; each Jadpo member is on its own line, and the formatter puts it there.",
        "Remove `,`";
    ASSERTION: SYNTAX_ASSERTION_FORM, Parser, "assert_eq!(a, b)" -> "assert a == b",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`assert_eq!` checks a test's values in Rust, `assertEquals` in JUnit, `assertEqual` in Python's unittest, `expect(…).toBe(…)` in Jest and `assert.equal` in Node; a Jadpo test checks with `assert condition`.",
        "Write `$J`";

    // --- layout and modules (area 1) -----------------------------------------
    HASH_COMMENT: SYNTAX_COMMENT_STYLE, Lexer, "#" -> "//",
        "Jadpo writes comments with `$J`, not `$W`", "write `$J`",
        "`#` starts a comment in Python, Ruby, shell scripts and YAML; a Jadpo comment starts with `//`, or `///` to document a declaration.",
        "Use `$J` instead of `$W`";
    BLOCK_COMMENT: SYNTAX_COMMENT_STYLE, Lexer, "/* */" -> "//",
        "Jadpo writes comments with `$J`, not `$W`", "write `$J`",
        "`/* */` encloses a comment in JavaScript, Rust, Go, Java, C#, Kotlin, Swift and SQL, and `/** */` documents a declaration; a Jadpo comment is `//` to the end of the line, and `///` documents.",
        "Write each line of the comment after `$J`";
    IMPORT: SYNTAX_IMPORT_STATEMENT, Parser, "import { A } from \"x\"" -> "use x { A }",
        "Jadpo writes `use`, not `import`", "write `use`",
        "`import … from` brings names into a JavaScript or TypeScript file, `from … import` into a Python module and `import a.b.C` into a Java or Kotlin file; a Jadpo file brings names in from a folder with `use <folder> { Names }`, and `jadpo fmt` writes those lines for you.",
        "$J";
    PATH_COLONS: SYNTAX_IMPORT_STATEMENT, Parser, "::" -> "/",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`::` separates a path in Rust; a Jadpo `use` line names a folder with `/`, as in `use shared/types { Amount }`.",
        "Write `$J`";
    EXPORT: SYNTAX_EXPORT_KEYWORD, Parser, "export" -> "",
        "Jadpo has no `$W`", "remove `$W`",
        "`export` makes a name visible to other files in JavaScript and TypeScript, and `pub` outside its module in Rust; every Jadpo declaration is visible across its domain, and `public` opens it to other domains.",
        "Remove `$W`";

    // --- values and types (area 2) -------------------------------------------
    SINGLE_QUOTES: SYNTAX_STRING_QUOTES, Lexer, "'text'" -> "\"text\"",
        "Jadpo writes strings in double quotes", "write `\"`",
        "Single quotes enclose text in Python, JavaScript and SQL, and a character in Rust, Java, C# and Kotlin, and backticks a template in JavaScript; Jadpo has one kind of string, in double quotes.",
        "Write `$J`";
    NEW: SYNTAX_NEW_KEYWORD, Parser, "new" -> "",
        "Jadpo creates a value without `new`", "remove `new`",
        "`new` creates an object in JavaScript, TypeScript, Java and C#; Jadpo creates a value with `Type(value)`, or an object with `Type { field: value }`.",
        "Remove `new`";
    STRUCT: SYNTAX_TYPE_KEYWORD, Parser, "struct T" -> "type T = Object",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`struct` declares a record in Rust, Go, Swift and C#, `interface` an object's shape in TypeScript, and `enum` a list of cases in most languages; every Jadpo type is `type Name = Base { ... }`, with `Object` or `Enum` as the base.",
        "Write `$J`";
    ARRAY_TYPE: SYNTAX_COLLECTION_TYPE, Parser, "T[]" -> "List<T>",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`T[]` is a list type in TypeScript, Java and C#, `[]T` in Go, and `list[T]` in Python; Jadpo writes a collection's types in angle brackets, as in `List<T>`, `Set<T>` and `Map<K, V>`.",
        "Write `$J`";
    NULL_UNION: SYNTAX_NULLABLE_TYPE, Parser, "T | null" -> "T?",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "`T | null` and `T | undefined` are types that may be missing in TypeScript, and `T | None` and `Optional[T]` in Python; Jadpo writes `T?`, which holds a `T` or `none`.",
        "Write `$J`";
    NULL_COALESCING: SYNTAX_NULL_COALESCING, Parser, "a ?? b" -> "match",
        "Jadpo reads a value that may be `none` with `match`, not `$W`", "write a `match`",
        "`??` gives a default for a missing value in JavaScript, TypeScript, C# and Swift, and `?:` in Kotlin; Jadpo reads a value that may be `none` with a `match` that handles both cases.",
        "Write $J";
    OPTIONAL_CHAINING: SYNTAX_OPTIONAL_CHAINING, Parser, "a?.b" -> "match",
        "Jadpo reads a value that may be `none` with `match`, not `?.`", "write a `match`",
        "`?.` reads a field of a value that may be missing in JavaScript, TypeScript, C#, Kotlin and Swift; Jadpo reads a value that may be `none` with a `match` that handles both cases.",
        "Write $J";
    FOREIGN_TYPE: TYPE_FOREIGN_TYPE, Checker, "string" -> "Text",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "$W",
        "Use `$J` instead of `$W`";
    FOREIGN_VALUE: TYPE_FOREIGN_VALUE, Checker, "null" -> "none",
        "Jadpo writes `$J`, not `$W`", "write `$J`",
        "$W",
        "Use `$J` instead of `$W`";
}

/// The variable-declaring word another language uses, if `word` is one.
pub(crate) fn variable_keyword(word: &str) -> Option<&'static Habit> {
    match word {
        "let" => Some(&LET),
        "const" => Some(&CONST),
        "val" => Some(&VAL),
        _ => None,
    }
}

/// The function-declaring word another language uses, if `word` is one.
pub(crate) fn function_keyword(word: &str) -> Option<&'static Habit> {
    match word {
        "fn" => Some(&FN),
        "func" => Some(&FUNC),
        "def" => Some(&DEF),
        "fun" => Some(&FUN),
        _ => None,
    }
}

/// `elif`, `elsif` or `elseif`.
pub(crate) fn is_else_if(word: &str) -> bool {
    matches!(word, "elif" | "elsif" | "elseif")
}

/// A word for a missing value in another language, as it may follow `|`
/// in a type or `is` in a comparison.
pub(crate) fn is_null_word(word: &str) -> bool {
    matches!(word, "null" | "undefined" | "None" | "NULL" | "nil")
}

/// A type name another language uses, with the languages it comes from and
/// its Jadpo name. A name the project declares is the project's own, so the
/// checker asks only for names that resolve to nothing.
pub fn foreign_type(name: &str) -> Option<(&'static str, &'static str)> {
    let found = match name {
        "string" => ("text in TypeScript, C# and Go", "Text"),
        "str" => ("text in Python", "Text"),
        "String" => ("text in Rust, Java, Kotlin and Swift", "Text"),
        "text" | "varchar" => ("text in SQL", "Text"),
        "int" => ("a whole number in Python, Java, C# and Go", "Int"),
        "integer" | "bigint" => ("a whole number in SQL", "Int"),
        "Integer" | "Long" => ("a whole number in Java and Kotlin", "Int"),
        "long" => ("a whole number in Java and C#", "Int"),
        "i32" | "i64" | "u32" | "u64" | "usize" | "isize" => ("a whole number in Rust", "Int"),
        "int32" | "int64" => ("a whole number in Go", "Int"),
        "bool" => ("true or false in Python, Rust, C# and Go", "Bool"),
        "boolean" => ("true or false in TypeScript, Java and SQL", "Bool"),
        "Boolean" => ("true or false in Java and Kotlin", "Bool"),
        "decimal" | "numeric" => ("an exact decimal in C# and SQL", "Decimal"),
        "BigDecimal" => ("an exact decimal in Java", "Decimal"),
        "uuid" => ("an identifier in SQL", "Uuid"),
        "UUID" => ("an identifier in Java and Swift", "Uuid"),
        "Guid" => ("an identifier in C#", "Uuid"),
        "bytes" => ("binary data in Python", "Bytes"),
        "bytea" => ("binary data in PostgreSQL", "Bytes"),
        "ByteArray" => ("binary data in Kotlin", "Bytes"),
        "Buffer" | "Uint8Array" => ("binary data in JavaScript", "Bytes"),
        "void" => ("nothing to return in TypeScript, Java and C#", "Unit"),
        "Void" => ("nothing to return in Java and Swift", "Unit"),
        "None" => ("nothing to return in Python", "Unit"),
        "Vec" => ("a list in Rust", "List"),
        "Array" => ("a list in TypeScript and Swift", "List"),
        "ArrayList" => ("a list in Java", "List"),
        "list" => ("a list in Python", "List"),
        "HashMap" => ("a map in Rust and Java", "Map"),
        "Dictionary" => ("a map in C# and Swift", "Map"),
        "dict" => ("a map in Python", "Map"),
        "Record" => ("a map in TypeScript", "Map"),
        "HashSet" => ("a set in Rust, Java and C#", "Set"),
        "set" => ("a set in Python", "Set"),
        "Option" => ("a value that may be missing in Rust", "?"),
        "Optional" => ("a value that may be missing in Java and Swift", "?"),
        "LocalDate" => ("a date in Java and Kotlin", "CalendarDate"),
        "LocalTime" => ("a time of day in Java and Kotlin", "Time"),
        "ZoneId" => ("a time zone in Java", "Zone"),
        _ => return None,
    };
    Some(found)
}

/// A value another language writes differently, with the languages it
/// comes from and its Jadpo spelling.
pub fn foreign_value(name: &str) -> Option<(&'static str, &'static str)> {
    let found = match name {
        "null" => ("a missing value in JavaScript, Java, C# and Kotlin", "none"),
        "NULL" => ("a missing value in SQL", "none"),
        "nil" => ("a missing value in Go, Swift and Ruby", "none"),
        "undefined" => ("a missing value in JavaScript", "none"),
        "None" => ("a missing value in Python", "none"),
        "True" => ("true in Python", "true"),
        "False" => ("false in Python", "false"),
        "TRUE" => ("true in SQL", "true"),
        "FALSE" => ("false in SQL", "false"),
        _ => return None,
    };
    Some(found)
}

/// The reason for a foreign type or value: `` `string` is text in
/// TypeScript, C# and Go; Jadpo calls it `Text`. ``
pub fn foreign_reason(written: &str, meaning: &str, jadpo: &str, what: &str) -> String {
    if jadpo == "?" {
        return format!(
            "`{written}` is {meaning}; Jadpo writes `T?`, which holds a `T` or `none`."
        );
    }
    format!("`{written}` is {meaning}; Jadpo {what} `{jadpo}`.")
}
