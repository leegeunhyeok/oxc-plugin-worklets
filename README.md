# oxc-plugin-worklets

A Rust port of React Native Reanimated's [worklets Babel plugin](https://github.com/software-mansion/react-native-reanimated/tree/main/packages/react-native-worklets/plugin), built on top of [oxc](https://oxc.rs/).

Designed to integrate with the [Rolldown](https://rolldown.rs/) bundler.

## Requirements

- [mise](https://mise.jdx.dev/) (manages Rust, Node, just)
- oxc 0.115.0 (pinned for Rolldown compatibility)

## Setup

```bash
mise install
```

## Development

```bash
just          # List available commands
just build    # Build
just test     # Run all tests
just lint     # Check fmt + clippy
just fmt      # Auto-fix formatting
just snapshot # Update insta snapshots
```

## Testing

```bash
just test # Run all tests
```

Integration tests live in `tests/transform.rs` and use [insta](https://insta.rs/) for snapshot testing. Snapshots are stored in `tests/snapshots/`.

## Release

```bash
just release 0.2.0
# → updates Cargo.toml version
# → commits "chore: release crates v0.2.0"
# → push to main triggers cargo publish via CI
```

## Usage

### API

```rust
use oxc::allocator::Allocator;
use oxc::ast::ast::Program;
use oxc_plugin_worklets::{WorkletsVisitor, PluginOptions};

fn example(allocator: &Allocator, program: &mut Program<'_>) {
    let opts = PluginOptions {
        filename: Some("/path/to/file.js".to_string()),
        is_release: true,
        plugin_version: "0.0.0", // Must be matched with the `react-native-worklets` runtime version
        ..Default::default()
    };

    let mut visitor = WorkletsVisitor::new(allocator, opts);
    visitor.visit_program(program).expect("transform failed");
}
```

## License

[MIT](./LICENSE)
