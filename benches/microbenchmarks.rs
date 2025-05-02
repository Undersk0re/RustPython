use criterion::{
    BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
    measurement::WallTime,
};
use pyo3::types::PyAnyMethods;
use rustpython_compiler::Mode;
use rustpython_vm::{AsObject, Interpreter, PyResult, Settings};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

// List of microbenchmarks to skip.
//
// These result in excessive memory usage, some more so than others. For example, while
// exception_context.py consumes a lot of memory, it still finishes. On the other hand,
// call_kwargs.py seems like it performs an excessive amount of allocations and results in
// a system freeze.
// In addition, the fact that we don't yet have a GC means that benchmarks which might consume
// a bearable amount of memory accumulate. As such, best to skip them for now.
const SKIP_MICROBENCHMARKS: [&str; 8] = [
    "call_simple.py",
    "call_kwargs.py",
    "construct_object.py",
    "define_function.py",
    "define_class.py",
    "exception_nested.py",
    "exception_simple.py",
    "exception_context.py",
];

// Struct representing a single microbenchmark
pub struct MicroBenchmark {
    name: String,     // Name of the benchmark (usually the filename)
    setup: String,    // Setup code to run before the main code (optional)
    code: String,     // The main code to benchmark
    iterate: bool,    // Whether to run the code multiple times with different iteration counts
}

// Run the benchmark using CPython (via PyO3)
fn bench_cpython_code(group: &mut BenchmarkGroup<WallTime>, bench: &MicroBenchmark) {
    pyo3::Python::with_gil(|py| {
        // Compile the setup and main code using Python's built-in compile function
        let setup_name = format!("{}_setup", bench.name);
        let setup_code = cpy_compile_code(py, &bench.setup, &setup_name).unwrap();
        let code = cpy_compile_code(py, &bench.code, &bench.name).unwrap();

        // Get the built-in exec function for running code objects
        let builtins =
            pyo3::types::PyModule::import(py, "builtins").expect("Failed to import builtins");
        let exec = builtins.getattr("exec").expect("no exec in builtins");

        // Function to run the main code in a given Python scope (globals/locals)
        let bench_func = |(globals, locals): &mut (
            pyo3::Bound<pyo3::types::PyDict>,
            pyo3::Bound<pyo3::types::PyDict>,
        )| {
            let res = exec.call((&code, &*globals, &*locals), None);
            if let Err(e) = res {
                e.print(py);
                panic!("Error running microbenchmark")
            }
        };

        // Function to set up the Python scope, optionally setting ITERATIONS
        let bench_setup = |iterations| {
            let globals = pyo3::types::PyDict::new(py);
            let locals = pyo3::types::PyDict::new(py);
            if let Some(idx) = iterations {
                globals.set_item("ITERATIONS", idx).unwrap();
            }
            let res = exec.call((&setup_code, &globals, &locals), None);
            if let Err(e) = res {
                e.print(py);
                panic!("Error running microbenchmark setup code")
            }
            (globals, locals)
        };

        // If the benchmark is iterative, run it with different iteration counts
        if bench.iterate {
            for idx in (100..=1_000).step_by(200) {
                group.throughput(Throughput::Elements(idx as u64));
                group.bench_with_input(BenchmarkId::new("cpython", &bench.name), &idx, |b, idx| {
                    b.iter_batched_ref(
                        || bench_setup(Some(*idx)),
                        bench_func,
                        BatchSize::LargeInput,
                    );
                });
            }
        } else {
            // Otherwise, just run it once
            group.bench_function(BenchmarkId::new("cpython", &bench.name), move |b| {
                b.iter_batched_ref(|| bench_setup(None), bench_func, BatchSize::LargeInput);
            });
        }
    })
}

// Helper function to compile Python code using PyO3
fn cpy_compile_code<'a>(
    py: pyo3::Python<'a>,
    code: &str,
    name: &str,
) -> pyo3::PyResult<pyo3::Bound<'a, pyo3::types::PyCode>> {
    let builtins =
        pyo3::types::PyModule::import(py, "builtins").expect("Failed to import builtins");
    let compile = builtins.getattr("compile").expect("no compile in builtins");
    compile.call1((code, name, "exec"))?.extract()
}

// Run the benchmark using RustPython
fn bench_rustpython_code(group: &mut BenchmarkGroup<WallTime>, bench: &MicroBenchmark) {
    // Set up RustPython interpreter settings
    let mut settings = Settings::default();
    settings.path_list.push("Lib/".to_string());
    settings.write_bytecode = false;
    settings.user_site_directory = false;

    // Initialize the interpreter with the standard library
    Interpreter::with_init(settings, |vm| {
        for (name, init) in rustpython_stdlib::get_module_inits() {
            vm.add_native_module(name, init);
        }
    })
    .enter(|vm| {
        // Compile the setup and main code
        let setup_code = vm
            .compile(&bench.setup, Mode::Exec, bench.name.to_owned())
            .expect("Error compiling setup code");
        let bench_code = vm
            .compile(&bench.code, Mode::Exec, bench.name.to_owned())
            .expect("Error compiling bench code");

        // Function to run the main code in a given scope
        let bench_func = |scope| {
            let res: PyResult = vm.run_code_obj(bench_code.clone(), scope);
            vm.unwrap_pyresult(res);
        };

        // Function to set up the scope, optionally setting ITERATIONS
        let bench_setup = |iterations| {
            let scope = vm.new_scope_with_builtins();
            if let Some(idx) = iterations {
                scope
                    .locals
                    .as_object()
                    .set_item("ITERATIONS", vm.new_pyobj(idx), vm)
                    .expect("Error adding ITERATIONS local variable");
            }
            let setup_result = vm.run_code_obj(setup_code.clone(), scope.clone());
            vm.unwrap_pyresult(setup_result);
            scope
        };

        // If the benchmark is iterative, run it with different iteration counts
        if bench.iterate {
            for idx in (100..=1_000).step_by(200) {
                group.throughput(Throughput::Elements(idx as u64));
                group.bench_with_input(
                    BenchmarkId::new("rustpython", &bench.name),
                    &idx,
                    |b, idx| {
                        b.iter_batched(
                            || bench_setup(Some(*idx)),
                            bench_func,
                            BatchSize::LargeInput,
                        );
                    },
                );
            }
        } else {
            // Otherwise, just run it once
            group.bench_function(BenchmarkId::new("rustpython", &bench.name), move |b| {
                b.iter_batched(|| bench_setup(None), bench_func, BatchSize::LargeInput);
            });
        }
    })
}

// Run both CPython and RustPython benchmarks for a given microbenchmark
pub fn run_micro_benchmark(c: &mut Criterion, benchmark: MicroBenchmark) {
    let mut group = c.benchmark_group("microbenchmarks");

    bench_cpython_code(&mut group, &benchmark);
    bench_rustpython_code(&mut group, &benchmark);

    group.finish();
}

// Main function to discover and run all microbenchmarks
pub fn criterion_benchmark(c: &mut Criterion) {
    // Find all files in the microbenchmarks directory
    let benchmark_dir = Path::new("./benches/microbenchmarks/");
    let dirs: Vec<fs::DirEntry> = benchmark_dir
        .read_dir()
        .unwrap()
        .collect::<io::Result<_>>()
        .unwrap();
    let paths: Vec<PathBuf> = dirs.iter().map(|p| p.path()).collect();

    // Parse each file into a MicroBenchmark struct
    let benchmarks: Vec<MicroBenchmark> = paths
        .into_iter()
        .map(|p| {
            let name = p.file_name().unwrap().to_os_string();
            let contents = fs::read_to_string(p).unwrap();
            let iterate = contents.contains("ITERATIONS");

            // If the file contains "# ---", split into setup and main code
            let (setup, code) = if contents.contains("# ---") {
                let split: Vec<&str> = contents.splitn(2, "# ---").collect();
                (split[0].to_string(), split[1].to_string())
            } else {
                ("".to_string(), contents)
            };
            let name = name.into_string().unwrap();
            MicroBenchmark {
                name,
                setup,
                code,
                iterate,
            }
        })
        .collect();

    // Run each benchmark, skipping those in the skip list
    for benchmark in benchmarks {
        if SKIP_MICROBENCHMARKS.contains(&benchmark.name.as_str()) {
            continue;
        }
        run_micro_benchmark(c, benchmark);
    }
}

// These macros register the benchmarks with Criterion
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
