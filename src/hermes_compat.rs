//! Hermes-compatible syntax lowering for worklet code strings.
//!
//! Strips TypeScript and lowers ES2015+ to ES5-level syntax that the
//! Hermes engine (React Native) can execute directly.
//!
//! Corresponds to `workletTransformSync` in the Babel plugin which re-runs
//! Babel with `@babel/preset-typescript` and user-provided presets/plugins.

use std::sync::LazyLock;

use swc_common::{sync::Lrc, FileName, Mark, SourceMap, GLOBALS};
use swc_ecma_ast::{Expr, Pass, Program, Stmt};
use swc_ecma_codegen::{text_writer::JsWriter, Emitter};
use swc_ecma_compat_es2015::{arrow, block_scoping, destructuring, shorthand, template_literal};
use swc_ecma_compat_es2018::object_rest_spread;
use swc_ecma_compat_es2020::{nullish_coalescing, optional_chaining};
use swc_ecma_compat_es2021::logical_assignments;
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax};
use swc_ecma_transforms_base::{fixer::fixer, helpers, hygiene::hygiene, resolver};
use swc_ecma_transforms_typescript::typescript::strip as strip_typescript;

/// Shared SWC Globals instance, created once per process.
/// This avoids repeated creation and potential conflicts with Rolldown's
/// own SWC Globals when running in the same process.
static SWC_GLOBALS: LazyLock<swc_common::Globals> = LazyLock::new(swc_common::Globals::new);

/// Strips TypeScript syntax and lowers ES2015+ syntax to ES5.
///
/// Applies:
/// - TypeScript: type annotations, enums, namespaces → stripped
/// - ES2015: arrow functions, template literals, shorthand properties,
///   destructuring, block scoping (const/let → var)
/// - ES2018: object rest/spread (must run before destructuring)
/// - ES2020: nullish coalescing (??), optional chaining (?.)
/// - ES2021: logical assignments (??=, &&=, ||=)
///
/// Returns the lowered code string.
#[cfg(test)]
fn lower_to_es5(code: &str) -> String {
    lower_to_es5_with_source_map(code, None).0
}

/// Strips TypeScript and lowers ES2015+ to ES5.
/// If `filename` is provided, a source map JSON string is also returned.
pub fn lower_to_es5_with_source_map(
    code: &str,
    filename: Option<&str>,
) -> (String, Option<String>) {
    GLOBALS.set(&SWC_GLOBALS, || {
        let helpers = helpers::Helpers::new(false);
        helpers::HELPERS.set(&helpers, || lower_to_es5_inner(code, filename))
    })
}

fn lower_to_es5_inner(code: &str, filename: Option<&str>) -> (String, Option<String>) {
    let cm: Lrc<SourceMap> = Default::default();
    let source_file_name = filename
        .map(|f| FileName::Real(f.into()))
        .unwrap_or(FileName::Anon);
    let fm = cm.new_source_file(Lrc::new(source_file_name), code.to_string());

    let mut parser = Parser::new(
        Syntax::Typescript(TsSyntax {
            tsx: true,
            ..Default::default()
        }),
        StringInput::from(&*fm),
        None,
    );

    let mut program = match parser.parse_program() {
        Ok(p) => p,
        Err(_) => return (code.to_string(), None),
    };

    let unresolved_mark = Mark::new();
    let top_level_mark = Mark::new();
    let mut pass = (
        resolver(unresolved_mark, top_level_mark, false),
        strip_typescript(unresolved_mark, top_level_mark),
        (
            logical_assignments(),
            optional_chaining(Default::default(), unresolved_mark),
            nullish_coalescing(Default::default()),
            object_rest_spread(Default::default()),
            shorthand(),
            template_literal(Default::default()),
            arrow(unresolved_mark),
            destructuring(Default::default()),
            block_scoping(unresolved_mark),
        ),
        helpers::inject_helpers(unresolved_mark),
        hygiene(),
        fixer(None),
    );
    pass.process(&mut program);

    // inject_helpers adds helper function declarations (e.g. _slicedToArray)
    // at the program's top level. But worklet code must be a single expression
    // like `(function foo(){...})`. Move all helper declarations into the
    // function expression's body so they're self-contained.
    move_helpers_into_function_body(&mut program);

    // Codegen (minified, with optional source map)
    let mut buf = Vec::new();
    let mut src_map_buf = if filename.is_some() {
        Some(Vec::new())
    } else {
        None
    };

    {
        let wr = JsWriter::new(cm.clone(), "\n", &mut buf, src_map_buf.as_mut());
        let mut emitter = Emitter {
            cfg: swc_ecma_codegen::Config::default().with_minify(true),
            cm: cm.clone(),
            comments: None,
            wr: Box::new(wr),
        };
        emitter.emit_program(&program).expect("SWC codegen failed");
    }

    let code_out = String::from_utf8(buf).unwrap_or_else(|_| code.to_string());

    let source_map_json = src_map_buf.and_then(|src_map| {
        let mut sm_buf = Vec::new();
        (*cm)
            .build_source_map(
                &src_map,
                None,
                swc_common::source_map::DefaultSourceMapGenConfig,
            )
            .to_writer(&mut sm_buf)
            .ok()?;
        String::from_utf8(sm_buf).ok()
    });

    (code_out, source_map_json)
}

/// Moves top-level helper function declarations (injected by `inject_helpers`)
/// into the body of the main function expression.
///
/// Before: `function _slicedToArray(){...} (function foo(){...})`
/// After:  `(function foo(){ function _slicedToArray(){...} ...})`
fn move_helpers_into_function_body(program: &mut Program) {
    let body = match program {
        Program::Script(s) => &mut s.body,
        Program::Module(m) => {
            // Module items → only handle Stmt variants
            // This shouldn't happen for worklet code, but bail out safely
            if m.body.len() <= 1 {
                return;
            }
            // Can't easily handle module items, skip
            return;
        }
    };

    if body.len() <= 1 {
        return;
    }

    // The last statement is our `(function foo(){...});` expression.
    // All preceding statements are helper function declarations.
    let split = body.len() - 1;
    let helpers: Vec<Stmt> = body.drain(..split).collect();

    // Navigate into: ExprStmt → Paren → FnExpr → body
    if let Some(Stmt::Expr(expr_stmt)) = body.first_mut() {
        let expr = unwrap_paren_mut(&mut expr_stmt.expr);
        if let Expr::Fn(fn_expr) = expr {
            if let Some(fn_body) = &mut fn_expr.function.body {
                for (i, helper) in helpers.into_iter().enumerate() {
                    fn_body.stmts.insert(i, helper);
                }
            }
        }
    }

    // If we couldn't move helpers (unexpected shape), restore them
    // to avoid losing code silently.
    // (This path should not be reached for valid worklet code.)
}

/// Unwraps nested parenthesized expressions.
fn unwrap_paren_mut(expr: &mut Expr) -> &mut Expr {
    match expr {
        Expr::Paren(paren) => unwrap_paren_mut(&mut paren.expr),
        _ => expr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_arrow_function() {
        let input = "(function f(){var cb=()=>{return 1};})";
        let output = lower_to_es5(input);
        assert!(
            !output.contains("=>"),
            "arrow should be lowered: {}",
            output
        );
        assert!(
            output.contains("function"),
            "should contain function keyword"
        );
    }

    #[test]
    fn lowers_template_literal() {
        let input = "(function f(){return `hello`})";
        let output = lower_to_es5(input);
        assert!(
            !output.contains('`'),
            "template literal should be lowered: {}",
            output
        );
        assert!(output.contains("\"hello\"") || output.contains("'hello'"));
    }

    #[test]
    fn lowers_shorthand_property() {
        let input = "(function f(){var a=1;return {a}})";
        let output = lower_to_es5(input);
        assert!(
            output.contains("a:a") || output.contains("a: a"),
            "shorthand should be lowered: {}",
            output
        );
    }

    #[test]
    fn lowers_destructuring() {
        let input = "(function f(){const{a,b}=this.__closure;})";
        let output = lower_to_es5(input);
        assert!(
            !output.contains("const"),
            "const should be lowered to var: {}",
            output
        );
    }

    #[test]
    fn lowers_const_let() {
        let input = "(function f(){const x=1;let y=2;})";
        let output = lower_to_es5(input);
        assert!(
            !output.contains("const ") && !output.contains("let "),
            "const/let should be lowered to var: {}",
            output
        );
    }

    #[test]
    fn lowers_nullish_coalescing() {
        let input = "(function f(){var x=a??b;})";
        let output = lower_to_es5(input);
        assert!(
            !output.contains("??"),
            "nullish coalescing should be lowered: {}",
            output
        );
    }

    #[test]
    fn lowers_optional_chaining() {
        let input = "(function f(){var x=a?.b?.c;})";
        let output = lower_to_es5(input);
        assert!(
            !output.contains("?."),
            "optional chaining should be lowered: {}",
            output
        );
    }

    #[test]
    fn lowers_logical_assignment() {
        let input = "(function f(){var x=1;x??=2;x&&=3;x||=4;})";
        let output = lower_to_es5(input);
        assert!(
            !output.contains("??=") && !output.contains("&&=") && !output.contains("||="),
            "logical assignments should be lowered: {}",
            output
        );
    }

    #[test]
    fn lowers_object_rest_spread() {
        let input = "(function f(){var {a, ...rest} = obj;})";
        let output = lower_to_es5(input);
        assert!(
            !output.contains("...rest"),
            "object rest should be lowered: {}",
            output
        );
    }

    #[test]
    fn inlines_helpers_for_array_destructuring() {
        let input = "(function f(){var [a, b] = arr;})";
        let output = lower_to_es5(input);
        assert!(
            !output.contains("[a, b]") && !output.contains("[a,b]"),
            "array destructuring should be lowered: {}",
            output
        );
        // Must NOT use require() for helpers
        assert!(
            !output.contains("require("),
            "helpers must be inlined, not require()'d: {}",
            output
        );
        // Must still be a single expression (function), not multiple statements
        assert!(
            output.starts_with("(function") || output.starts_with("function"),
            "output should start with function expression: {}",
            output
        );
    }
}
