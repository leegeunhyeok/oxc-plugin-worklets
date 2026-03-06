# React Native Reanimated Plugin — Rust Porting Project

## Project Structure

- This is a fresh project. Start by creating a new Cargo library crate.
- The original plugin implementation can be found in the GitHub submodule at `react-native-reanimated/*`:
  - Source: `<submodule>/packages/react-native-worklets/plugin/**/*`
  - Existing Babel plugin tests are available and should serve as the reference for expected behavior.

---

## Requirements

### Core Goal

Port the `react-native-worklets` Babel plugin to Rust using the `oxc` ecosystem.

> **oxc crate versions must be pinned to the versions listed in the "Crate Versions" section below**, as these must remain compatible with the Rolldown bundler.

### Testing

- Write tests to verify behavioral parity with the original implementation.
- Test cases can be derived from the existing test suite inside the submodule.
- Snapshot tests may not match 100% due to inherent differences between Babel and oxc output:
  - Mismatches caused by **code indentation** or **identifier naming** differences are acceptable — update snapshots to reflect oxc-based output in these cases.
  - All **business logic behavior** must be 100% compatible with the original.

#### Test Infrastructure

Use a **fixture-based snapshot testing** approach that mirrors the structure of the original Babel test suite.

**Recommended crate:** [`insta`](https://crates.io/crates/insta) for snapshot management.

**Snapshot update policy:**

- Run `cargo insta review` to review and accept snapshot diffs interactively.
- Only accept diffs that are clearly cosmetic (indentation, identifier casing). Reject and fix diffs that reflect logic differences.
- Accepted oxc-based snapshots are the source of truth going forward; do not attempt to match Babel's exact output byte-for-byte.

### Public Interface

The crate must expose a `transform` function with the following signature. Arguments are passed as individual parameters rather than as a composite type, so that this crate depends only on `oxc` with no dependency on Rolldown or any Rolldown-internal types.

```rust
use oxc::{allocator::Allocator, ast::ast::Program};

pub struct WorkletsVisitor<'a> {
    // ...
}

impl<'a> WorkletsVisitor<'a> {
    pub fn new(allocator: &'a Allocator, options: WorkletsOptions) -> Self {
        // ...
    }

    pub fn visit_program(&mut self, program: &mut Program<'a>) -> Result<(), WorkletsError> {
        // Apply the same transformations as the Babel plugin
    }
}
```

#### Integration with Rolldown

The final goal is to integrate this crate into Rolldown as a native plugin. The integration glue lives on the Rolldown side — this crate has no knowledge of Rolldown internals.

```rust
use rolldown_plugin::{Plugin, PluginContext, HookTransformAstArgs, HookTransformAstReturn};

impl Plugin for ReactNativeWorkletsPlugin {
    async fn transform_ast(
        &self,
        ctx: &PluginContext,
        mut args: HookTransformAstArgs<'_>,
    ) -> HookTransformAstReturn {
        args.ast.program.with_mut(|fields| {
            let mut visitor = WorkletsVisitor::new(fields.allocator, WorkletsOptions::default());
            visitor.visit_program(fields.program)?;
            Ok(())
        })?;
        Ok(args.ast)
    }
}
```

### Exceptions

- **Flow syntax is out of scope.** Drop support for Flow entirely; TypeScript and JavaScript only.
  - When Flow syntax is encountered, return an error rather than panicking. The `transform` function's return type may be changed to `Result` to accommodate this.

**Guidelines:**

- **Do not panic.** All recoverable error conditions must be surfaced via `Result`. Reserve `unreachable!()` or `panic!()` only for invariants that are genuinely impossible to violate.

---

### Maintainability

This project must remain sustainable over time. When the upstream Babel plugin changes, those changes must be tracked and ported to Rust accordingly.

To make this as straightforward as possible, **mirror the upstream structure as closely as Rust conventions allow**:

- Keep function names, file names, and module names aligned with the original implementation.
- When the upstream adds, removes, or renames a function or file, the corresponding change in this crate should be easy to locate and apply.
- Where a direct mapping is not possible (e.g. due to language differences), leave a comment referencing the upstream counterpart:

```rust
// Corresponds to `processWorkletFunction` in
// packages/react-native-worklets/plugin/src/worklet.ts
fn process_worklet_function(...) { ... }
```

> **NOTE:** If there are any ambiguities or areas requiring clarification beyond what is specified here, please ask before proceeding.

---

## Crate Versions

The following versions must be used as-is to maintain compatibility with Rolldown:

```toml
oxc = { version = "0.115.0" }
oxc_allocator = { version = "0.115.0" }
oxc_ecmascript = { version = "0.115.0" }
oxc_napi = { version = "0.115.0" }
oxc_minify_napi = { version = "0.115.0" }
oxc_parser_napi = { version = "0.115.0" }
oxc_transform_napi = { version = "0.115.0" }
oxc_traverse = { version = "0.115.0" }
oxc_index = { version = "4" }
oxc_resolver = { version = "11.17.1" }
oxc_resolver_napi = { version = "11.17.1" }
oxc_sourcemap = { version = "6" }
```
