use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use llvm_backend::{compile, OptimizationLevelSetting};

fn build_source(name: &str) -> &'static str {
    match name {
        "simple_function" => {
            r#"
            func add(a: i32, b: i32) -> i32 {
                return a + b
            }

            func main() -> i32 {
                return add(20, 22)
            }
            "#
        }
        "milestone_program" => {
            r#"
            func add(a: i32, b: i32) -> i32 {
                return a + b
            }

            func main() -> i32 {
                val result = add(5, 3)
                return result
            }
            "#
        }
        "factorial_program" => {
            r#"
            func factorial(n: i32) -> i32 {
                if n <= 1 {
                    return 1
                } else {
                    return n * factorial(n - 1)
                }
                return 1
            }

            func main() -> i32 {
                return factorial(5)
            }
            "#
        }
        _ => unreachable!("unknown benchmark program"),
    }
}

fn bench_codegen(c: &mut Criterion) {
    let mut group = c.benchmark_group("codegen");

    for case in ["simple_function", "milestone_program", "factorial_program"] {
        let source = build_source(case);
        let items = syntax_parsing::parse(source).expect("benchmark source must parse");

        group.bench_with_input(BenchmarkId::new("compile", case), &items, |b, parsed_items| {
            b.iter(|| {
                let result = compile(parsed_items, OptimizationLevelSetting::O2);
                assert!(result.is_ok(), "benchmark compilation failed: {:?}", result.err());
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_codegen);
criterion_main!(benches);