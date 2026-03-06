use oxc::allocator::Allocator;
use oxc::parser::Parser;
use oxc::span::SourceType;

use oxc_plugin_worklets::{PluginOptions, WorkletsVisitor};

fn run_plugin(input: &str) -> String {
    run_plugin_with_opts(input, PluginOptions::default(), "/dev/null")
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
