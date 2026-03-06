use oxc::allocator::Allocator;
use oxc::parser::Parser;
use oxc::span::SourceType;

use oxc_plugin_worklets::{PluginOptions, WorkletsVisitor};

fn default_opts() -> PluginOptions {
    PluginOptions {
        plugin_version: "x.y.z".to_string(),
        ..Default::default()
    }
}

fn run_plugin(input: &str) -> String {
    run_plugin_with_opts(input, default_opts(), "/dev/null")
}

fn run_plugin_ts(input: &str) -> String {
    let mut opts = default_opts();
    opts.filename = Some("/dev/null".to_string());
    let allocator = Allocator::default();
    let source_type = SourceType::tsx();
    let ret = Parser::new(&allocator, input, source_type).parse();
    assert!(ret.errors.is_empty(), "Parse errors: {:?}", ret.errors);

    let mut program = ret.program;
    let mut visitor = WorkletsVisitor::new(&allocator, opts);
    visitor
        .visit_program(&mut program)
        .expect("transform should succeed");

    let codegen = oxc::codegen::Codegen::new();
    codegen.build(&program).code
}

fn run_plugin_with_opts(input: &str, mut opts: PluginOptions, filename: &str) -> String {
    opts.filename = Some(filename.to_string());
    let allocator = Allocator::default();
    let source_type = SourceType::mjs();
    let ret = Parser::new(&allocator, input, source_type).parse();
    assert!(ret.errors.is_empty(), "Parse errors: {:?}", ret.errors);

    let mut program = ret.program;
    let mut visitor = WorkletsVisitor::new(&allocator, opts);
    visitor
        .visit_program(&mut program)
        .expect("transform should succeed");

    let codegen = oxc::codegen::Codegen::new();
    codegen.build(&program).code
}

// --- Basic worklet directive tests ---

#[test]
fn test_simple_worklet_function() {
    let input = r#"
function foo() {
    'worklet';
    return 1;
}
"#;
    let output = run_plugin(input);
    // Should contain init_data declaration
    assert!(
        output.contains("_worklet_"),
        "should have worklet init data: {}",
        output
    );
    // Should contain __workletHash
    assert!(
        output.contains("__workletHash"),
        "should have worklet hash: {}",
        output
    );
    // Should contain __closure
    assert!(
        output.contains("__closure"),
        "should have closure: {}",
        output
    );
    // Original 'worklet' directive should be removed
    assert!(
        !output.contains("'worklet'"),
        "worklet directive should be removed: {}",
        output
    );
    // Should be wrapped in a const declaration
    assert!(
        output.contains("const foo"),
        "should be const declaration: {}",
        output
    );
    insta::assert_snapshot!("simple_worklet_function", output);
}

#[test]
fn test_simple_worklet_arrow() {
    let input = r#"
const foo = () => {
    'worklet';
    return 1;
};
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("_worklet_"),
        "should have worklet init data: {}",
        output
    );
    assert!(
        output.contains("__workletHash"),
        "should have worklet hash: {}",
        output
    );
    insta::assert_snapshot!("simple_worklet_arrow", output);
}

#[test]
fn test_worklet_with_closure() {
    let input = r#"
const a = 1;
function foo(x) {
    'worklet';
    return x + a;
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__closure"),
        "should have closure: {}",
        output
    );
    // The closure should capture 'a'
    assert!(
        output.contains("{ a }") || output.contains("a:"),
        "should capture 'a' in closure: {}",
        output
    );
    insta::assert_snapshot!("worklet_with_closure", output);
}

#[test]
fn test_worklet_recursive_function() {
    let input = r#"
const a = 1;
function foo(t) {
    'worklet';
    if (t > 0) {
        return a + foo(t - 1);
    }
}
"#;
    let output = run_plugin(input);
    // Should have _recur for recursive calls
    assert!(output.contains("_recur"), "should have _recur: {}", output);
    insta::assert_snapshot!("worklet_recursive", output);
}

#[test]
fn test_worklet_async_function() {
    let input = r#"
async function foo() {
    'worklet';
    await bar();
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("_worklet_"),
        "should have worklet: {}",
        output
    );
    insta::assert_snapshot!("worklet_async", output);
}

// --- Auto-workletization tests ---

#[test]
fn test_auto_workletize_use_animated_style() {
    let input = r#"
import { useAnimatedStyle } from 'react-native-reanimated';
const style = useAnimatedStyle(() => {
    return { opacity: 1 };
});
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "should auto-workletize: {}",
        output
    );
    insta::assert_snapshot!("auto_workletize_useAnimatedStyle", output);
}

#[test]
fn test_auto_workletize_use_derived_value() {
    let input = r#"
import { useDerivedValue } from 'react-native-reanimated';
const val = useDerivedValue(() => {
    return 42;
});
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "should auto-workletize: {}",
        output
    );
    insta::assert_snapshot!("auto_workletize_useDerivedValue", output);
}

// --- File-level worklet directive ---

#[test]
fn test_file_level_worklet_directive() {
    let input = r#"
'worklet';
function foo() {
    return 1;
}
function bar() {
    return 2;
}
"#;
    let output = run_plugin(input);
    // Both functions should be workletized
    assert!(
        output.contains("__workletHash"),
        "should workletize all functions: {}",
        output
    );
    // Count init_data declarations - should be 2
    let count = output.matches("_worklet_").count();
    assert!(
        count >= 2,
        "should have at least 2 worklet init_data declarations, got {}: {}",
        count,
        output
    );
    insta::assert_snapshot!("file_level_worklet", output);
}

// --- Worklet names ---

#[test]
fn test_worklet_name_with_filename() {
    let input = r#"
function foo() {
    'worklet';
    return 1;
}
"#;
    let output = run_plugin_with_opts(
        input,
        PluginOptions {
            is_release: true,
            ..default_opts()
        },
        "/source.js",
    );
    // Should contain function name + source file name
    assert!(
        output.contains("foo_source_js"),
        "should have source file in name: {}",
        output
    );
    insta::assert_snapshot!("worklet_name_with_filename", output);
}

#[test]
fn test_worklet_name_with_library() {
    let input = r#"
function foo() {
    'worklet';
    return 1;
}
"#;
    let output = run_plugin_with_opts(
        input,
        PluginOptions {
            is_release: true,
            ..default_opts()
        },
        "/node_modules/library/source.js",
    );
    assert!(
        output.contains("foo_library_source_js"),
        "should have library name: {}",
        output
    );
    insta::assert_snapshot!("worklet_name_with_library", output);
}

// --- Export handling ---

#[test]
fn test_export_default_worklet() {
    let input = r#"
export default function foo() {
    'worklet';
    return 1;
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("export default"),
        "should keep export default: {}",
        output
    );
    assert!(
        output.contains("__workletHash"),
        "should workletize: {}",
        output
    );
    insta::assert_snapshot!("export_default_worklet", output);
}

#[test]
fn test_export_named_worklet() {
    let input = r#"
export function foo() {
    'worklet';
    return 1;
}
"#;
    let output = run_plugin(input);
    assert!(output.contains("export"), "should keep export: {}", output);
    assert!(
        output.contains("__workletHash"),
        "should workletize: {}",
        output
    );
    insta::assert_snapshot!("export_named_worklet", output);
}

// --- Gesture handler auto-workletization ---

#[test]
fn test_gesture_handler_workletization() {
    let input = r#"
import { Gesture } from 'react-native-gesture-handler';
const g = Gesture.Tap().onEnd(() => {
    console.log('tapped');
});
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "should auto-workletize gesture handler: {}",
        output
    );
    insta::assert_snapshot!("gesture_handler_workletization", output);
}

// --- Nested worklets ---

#[test]
fn test_nested_worklets() {
    let input = r#"
function outer() {
    'worklet';
    const inner = () => {
        'worklet';
        return 1;
    };
    return inner;
}
"#;
    let output = run_plugin(input);
    // Both functions should be workletized
    let count = output.matches("__workletHash").count();
    assert!(
        count >= 2,
        "should have at least 2 worklet hashes: {}",
        output
    );
    insta::assert_snapshot!("nested_worklets", output);
}

// --- Release mode ---

#[test]
fn test_release_mode() {
    let input = r#"
function foo() {
    'worklet';
    return 1;
}
"#;
    let output = run_plugin_with_opts(
        input,
        PluginOptions {
            is_release: true,
            ..default_opts()
        },
        "/dev/null",
    );
    // Should NOT contain __pluginVersion or __stackDetails in release mode
    assert!(
        !output.contains("__pluginVersion"),
        "should not have plugin version in release: {}",
        output
    );
    assert!(
        !output.contains("__stackDetails"),
        "should not have stack details in release: {}",
        output
    );
    insta::assert_snapshot!("release_mode", output);
}

// --- Context Object tests ---

#[test]
fn test_context_object_removes_marker() {
    let input = r#"
const foo = {
    bar() {
        return 'bar';
    },
    __workletContextObject: true,
};
"#;
    let output = run_plugin(input);
    assert!(
        !output.contains("__workletContextObject:"),
        "marker should be removed: {}",
        output
    );
    assert!(
        output.contains("__workletContextObjectFactory"),
        "should have factory: {}",
        output
    );
    insta::assert_snapshot!("context_object_removes_marker", output);
}

#[test]
fn test_context_object_creates_factory() {
    let input = r#"
const foo = {
    bar() {
        return 'bar';
    },
    __workletContextObject: true,
};
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletContextObjectFactory"),
        "should have factory: {}",
        output
    );
    assert!(
        output.contains("__workletHash"),
        "factory function should be workletized: {}",
        output
    );
    insta::assert_snapshot!("context_object_creates_factory", output);
}

#[test]
fn test_context_object_preserves_bindings() {
    let input = r#"
const foo = {
    bar() {
        return 'bar';
    },
    foobar() {
        return this.bar();
    },
    __workletContextObject: true,
};
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("this.bar()"),
        "should preserve this.bar(): {}",
        output
    );
    insta::assert_snapshot!("context_object_preserves_bindings", output);
}

// --- Referenced Worklet tests ---

#[test]
fn test_referenced_arrow_variable_declarator() {
    let input = r#"
let styleFactory = () => ({});
const animatedStyle = useAnimatedStyle(styleFactory);
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "should workletize referenced function: {}",
        output
    );
    insta::assert_snapshot!("referenced_arrow_var_declarator", output);
}

#[test]
fn test_referenced_function_expression_variable() {
    let input = r#"
let styleFactory = function() {
    return {};
};
const animatedStyle = useAnimatedStyle(styleFactory);
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "should workletize referenced function: {}",
        output
    );
    insta::assert_snapshot!("referenced_func_expr_var", output);
}

#[test]
fn test_referenced_function_declaration() {
    let input = r#"
function styleFactory() {
    return {};
}
const animatedStyle = useAnimatedStyle(styleFactory);
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "should workletize referenced function: {}",
        output
    );
    insta::assert_snapshot!("referenced_func_declaration", output);
}

#[test]
fn test_referenced_object_variable() {
    let input = r#"
let handler = {
    onScroll: () => {},
};
const scrollHandler = useAnimatedScrollHandler(handler);
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "should workletize referenced object: {}",
        output
    );
    insta::assert_snapshot!("referenced_object_var", output);
}

// --- File-level worklet with object methods ---

#[test]
fn test_file_level_worklet_object_methods() {
    let input = r#"
'worklet';
const foo = {
    bar() {
        return 'bar';
    },
};
"#;
    let output = run_plugin(input);
    // Object without `this` usage → methods are individually workletized (no context object)
    assert!(
        output.contains("__workletHash"),
        "should workletize methods: {}",
        output
    );
    insta::assert_snapshot!("file_level_worklet_object_methods", output);
}

#[test]
fn test_file_level_worklet_implicit_context_object() {
    let input = r#"
'worklet';
const foo = {
    bar() {
        return 'bar';
    },
    foobar() {
        return this.bar();
    },
};
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletContextObjectFactory"),
        "should have factory: {}",
        output
    );
    insta::assert_snapshot!("file_level_worklet_implicit_context_object", output);
}

// --- Worklet Class tests ---

#[test]
fn test_worklet_class_basic() {
    let input = r#"
class Foo {
    __workletClass = true;
    bar() {
        return 1;
    }
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__classFactory"),
        "should have classFactory: {}",
        output
    );
    assert!(
        output.contains("\"worklet\""),
        "factory should have worklet directive: {}",
        output
    );
    assert!(
        !output.contains("__workletClass"),
        "marker should be removed: {}",
        output
    );
    assert!(
        output.contains("const Foo"),
        "should have const declaration: {}",
        output
    );
    insta::assert_snapshot!("worklet_class_basic", output);
}

#[test]
fn test_worklet_class_file_level() {
    let input = r#"
'worklet';
class Foo {
    bar() {
        return 1;
    }
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__classFactory"),
        "should have classFactory: {}",
        output
    );
    assert!(
        output.contains("\"worklet\""),
        "factory should have worklet directive: {}",
        output
    );
    assert!(
        output.contains("const Foo"),
        "should have const declaration: {}",
        output
    );
    insta::assert_snapshot!("worklet_class_file_level", output);
}

#[test]
fn test_worklet_class_disabled() {
    let input = r#"
class Foo {
    __workletClass = true;
    bar() {
        return 1;
    }
}
"#;
    let output = run_plugin_with_opts(
        input,
        PluginOptions {
            disable_worklet_classes: true,
            ..default_opts()
        },
        "/dev/null",
    );
    // Should NOT be transformed when disabled
    assert!(
        output.contains("class Foo"),
        "class should remain: {}",
        output
    );
    assert!(
        !output.contains("__classFactory"),
        "should not have classFactory: {}",
        output
    );
}

// --- Var hoisting in closure analysis ---

#[test]
fn test_var_in_for_loop_not_captured() {
    // Simulates Rolldown's rest parameter transformation:
    // ...args becomes for(var args = ...) which is function-scoped via var hoisting
    let input = r#"
function foo(fn) {
    'worklet';
    for (var _len = arguments.length, args = new Array(_len > 1 ? _len - 1 : 0), _key = 1; _key < _len; _key++) {
        args[_key - 1] = arguments[_key];
    }
    return fn.apply(void 0, args);
}
"#;
    let output = run_plugin(input);
    // args, _len, _key are var-declared inside for loop — should NOT be captured
    assert!(
        !output.contains("\"args\"") && !output.contains("args:"),
        "args should not be in closure: {}",
        output
    );
    insta::assert_snapshot!("var_in_for_loop_not_captured", output);
}

#[test]
fn test_var_in_catch_block_not_captured() {
    // var declarations inside catch/if blocks are hoisted to function scope
    let input = r#"
function foo() {
    'worklet';
    try {
        doSomething();
    } catch (error) {
        if (globalThis.handler) {
            var message = error.message, stack = error.stack;
            globalThis.handler(message, stack);
        }
    }
}
"#;
    let output = run_plugin(input);
    // message, stack are var-declared inside catch/if — should NOT be captured
    assert!(
        !output.contains("\"message\"") && !output.contains("message:"),
        "message should not be in closure: {}",
        output
    );
    assert!(
        !output.contains("\"stack\"") && !output.contains("stack:"),
        "stack should not be in closure: {}",
        output
    );
    insta::assert_snapshot!("var_in_catch_block_not_captured", output);
}

#[test]
fn test_var_in_if_block_not_captured() {
    let input = r#"
function foo(x) {
    'worklet';
    if (x > 0) {
        var result = x * 2;
    }
    return result;
}
"#;
    let output = run_plugin(input);
    assert!(
        !output.contains("\"result\"") && !output.contains("result:"),
        "result should not be in closure: {}",
        output
    );
    insta::assert_snapshot!("var_in_if_block_not_captured", output);
}

// --- Nested worklet inside non-worklet function ---

#[test]
fn test_worklet_inside_non_worklet_function() {
    // Simulates Rolldown output where a worklet is inside an if block
    // inside a regular (non-worklet) function
    let input = r#"
function initializeRNRuntime() {
    if (true) {
        var testWorklet = function testWorklet() {
            "worklet";
        };
        if (!isWorkletFunction(testWorklet)) {
            throw new Error("fail");
        }
    }
    registerReportFatalRemoteError();
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "nested worklet should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_inside_non_worklet_function", output);
}

#[test]
fn worklet_in_call_argument() {
    let input = r#"
function runOnUISync(worklet) {}
function createSerializable(fn) {}

const result = runOnUISync(createSerializable(function() {
    'worklet';
    return 42;
}));
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "worklet in call argument should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_in_call_argument", output);
}

#[test]
fn worklet_arrow_in_call_argument() {
    let input = r#"
const result = someCall(wrapFn(() => {
    'worklet';
    return 1 + 2;
}));
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "worklet arrow in call argument should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_arrow_in_call_argument", output);
}

#[test]
fn worklet_inside_try_catch() {
    let input = r#"
function init() {
    try {
        const w = function() {
            'worklet';
            return 1;
        };
    } catch (e) {
        const handler = () => {
            'worklet';
            return 2;
        };
    }
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "worklet inside try/catch should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_inside_try_catch", output);
}

#[test]
fn worklet_inside_for_loop() {
    let input = r#"
function setup() {
    for (let i = 0; i < 10; i++) {
        const w = function() {
            'worklet';
            return i;
        };
    }
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "worklet inside for loop should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_inside_for_loop", output);
}

#[test]
fn worklet_inside_switch() {
    let input = r#"
function handle(type) {
    switch (type) {
        case 'a':
            const w = function() { 'worklet'; return 1; };
            break;
    }
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "worklet inside switch should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_inside_switch", output);
}

#[test]
fn worklet_with_await() {
    let input = r#"
async function init() {
    const result = await createWorklet(function() {
        'worklet';
        return 42;
    });
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "worklet inside await should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_with_await", output);
}

#[test]
fn worklet_strips_typescript_types() {
    let input = r#"
function callGuard<Args extends unknown[], ReturnValue>(
    fn: (...args: Args) => ReturnValue,
    ...args: Args
): ReturnValue | void {
    'worklet';
    try {
        return fn(...args);
    } catch (error) {
        const { message, stack } = error as Error;
        console.log(message, stack ?? '');
    }
}
"#;
    let output = run_plugin_ts(input);
    // Extract the code string from init_data (the string inside `code: "..."`)
    let code_start = output.find("code: \"").expect("should have code field") + 7;
    let code_end = output[code_start..]
        .find("\",")
        .or_else(|| output[code_start..].find("\""))
        .expect("should have closing quote")
        + code_start;
    let code_string = &output[code_start..code_end];

    // TS types should be stripped from the worklet code string
    assert!(
        !code_string.contains("Args"),
        "TypeScript types should be stripped from worklet code string: {}",
        code_string
    );
    assert!(
        !code_string.contains("ReturnValue"),
        "TypeScript return type should be stripped: {}",
        code_string
    );
    assert!(
        !code_string.contains(" as Error"),
        "TypeScript 'as' assertion should be stripped: {}",
        code_string
    );
    // Nullish coalescing should be lowered
    assert!(
        !code_string.contains("??"),
        "Nullish coalescing should be lowered in worklet code string: {}",
        code_string
    );
    insta::assert_snapshot!("worklet_strips_typescript", output);
}

#[test]
fn worklet_in_constructor_function() {
    // Simulates class constructor lowered to a regular function
    let input = r#"
function NativeReanimatedModule() {
    this.workletsModule = WorkletsModule;
    if (__DEV__) {
        assertSingleReanimatedInstance();
    }
    runOnUISync(function initializeUI() {
        'worklet';
        registerReanimatedError();
    });
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "worklet inside constructor function should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_in_constructor_function", output);
}

#[test]
fn worklet_in_class_lowered_iife() {
    // Class lowered to IIFE pattern by bundler
    let input = r#"
var NativeReanimatedModule = /* @__PURE__ */ function() {
    function NativeReanimatedModule() {
        this.workletsModule = WorkletsModule;
        if (__DEV__) {
            assertSingleReanimatedInstance();
        }
        runOnUISync(function initializeUI() {
            "worklet";
            registerReanimatedError();
        });
    }
    return NativeReanimatedModule;
}();
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "worklet inside class-lowered IIFE should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_in_class_lowered_iife", output);
}

#[test]
fn worklet_in_class_constructor() {
    let input = r#"
class NativeModule {
    constructor() {
        runOnUISync(function initializeUI() {
            'worklet';
            registerError();
        });
    }
    someMethod() {
        return 1;
    }
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "worklet inside class constructor should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_in_class_constructor", output);
}

#[test]
fn worklet_in_top_level_if() {
    let input = r#"
if (!SHOULD_BE_USE_WEB) {
    runOnUISync(() => {
        'worklet';
        global._tagToJSPropNamesMapping = {};
    });
}
"#;
    let output = run_plugin(input);
    assert!(
        output.contains("__workletHash"),
        "worklet inside top-level if should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_in_top_level_if", output);
}

#[test]
fn worklet_in_ts_as_expression() {
    let input = r#"
function cloneRegExp(value) {
    const pattern = value.source;
    const flags = value.flags;
    const handle = cloneInitializer({
        __init: () => {
            'worklet';
            return new RegExp(pattern, flags);
        },
    }) as unknown as SerializableRef;
    return handle;
}
"#;
    let output = run_plugin_ts(input);
    assert!(
        output.contains("__workletHash"),
        "worklet inside TS as expression should be transformed: {}",
        output
    );
    insta::assert_snapshot!("worklet_in_ts_as_expression", output);
}
