use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use oxc::allocator::Allocator;
use oxc::codegen::Codegen;
use oxc::parser::Parser;
use oxc::span::SourceType;

use oxc_react_native_worklets::{WorkletsOptions, WorkletsVisitor};

fn main() {
    let n: u32 = env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let fixture_path = env::args().nth(2).map(PathBuf::from).unwrap_or_else(|| {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest_dir).join("../fixture.ts")
    });

    let source = fs::read_to_string(&fixture_path).expect("failed to read fixture");
    let source_type = SourceType::tsx();

    let opts = WorkletsOptions {
        plugin_version: "bench".to_string(),
        filename: Some("fixture.ts".to_string()),
        ..Default::default()
    };

    // Warmup
    {
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, &source, source_type).parse();
        assert!(ret.errors.is_empty(), "Parse errors: {:?}", ret.errors);
        let mut program = ret.program;
        let mut visitor = WorkletsVisitor::new(&allocator, opts.clone());
        visitor
            .visit_program(&mut program)
            .expect("transform failed");
        let _ = Codegen::new().build(&program).code;
    }

    let start = Instant::now();
    for _ in 0..n {
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, &source, source_type).parse();
        let mut program = ret.program;
        let mut visitor = WorkletsVisitor::new(&allocator, opts.clone());
        visitor
            .visit_program(&mut program)
            .expect("transform failed");
        let _ = Codegen::new().build(&program).code;
    }
    let elapsed = start.elapsed();
    let total_ms = elapsed.as_secs_f64() * 1000.0;

    println!("oxc: {} iterations", n);
    println!("  total: {:.2} ms", total_ms);
    println!("  avg:   {:.2} ms/transform", total_ms / n as f64);
}
