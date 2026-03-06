use std::collections::HashSet;

const NOT_CAPTURED_IDENTIFIERS: &[&str] = &[
    // Value properties
    "globalThis",
    "Infinity",
    "NaN",
    "undefined",
    // Function properties
    "eval",
    "isFinite",
    "isNaN",
    "parseFloat",
    "parseInt",
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "escape",
    "unescape",
    // Fundamental objects
    "Object",
    "Function",
    "Boolean",
    "Symbol",
    // Error objects
    "Error",
    "AggregateError",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "InternalError",
    // Numbers and dates
    "Number",
    "BigInt",
    "Math",
    "Date",
    // Text processing
    "String",
    "RegExp",
    // Indexed collections
    "Array",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "BigInt64Array",
    "BigUint64Array",
    "Float32Array",
    "Float64Array",
    // Keyed collections
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    // Structured data
    "ArrayBuffer",
    "SharedArrayBuffer",
    "DataView",
    "Atomics",
    "JSON",
    // Managing memory
    "WeakRef",
    "FinalizationRegistry",
    // Control abstraction objects
    "Iterator",
    "AsyncIterator",
    "Promise",
    "GeneratorFunction",
    "AsyncGeneratorFunction",
    "Generator",
    "AsyncGenerator",
    "AsyncFunction",
    // Reflection
    "Reflect",
    "Proxy",
    // Internationalization
    "Intl",
    // Other
    "null",
    "this",
    "global",
    "window",
    "globalThis",
    "self",
    "console",
    "performance",
    "arguments",
    "require",
    "fetch",
    "XMLHttpRequest",
    "WebSocket",
    // Run loop
    "queueMicrotask",
    "requestAnimationFrame",
    "cancelAnimationFrame",
    "setTimeout",
    "clearTimeout",
    "setImmediate",
    "clearImmediate",
    "setInterval",
    "clearInterval",
    // Hermes
    "HermesInternal",
    // Worklets
    "_WORKLET",
    // Deprecated
    "_IS_FABRIC",
];

pub fn build_globals(custom_globals: &[String]) -> HashSet<String> {
    let mut set: HashSet<String> = NOT_CAPTURED_IDENTIFIERS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    for g in custom_globals {
        set.insert(g.clone());
    }
    set
}
