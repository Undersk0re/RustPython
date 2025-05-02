use criterion::measurement::WallTime;
use criterion::{
    Bencher, BenchmarkGroup, BenchmarkId, Criterion, Throughput, black_box, criterion_group,
    criterion_main,
};
use rustpython_compiler::Mode;
use rustpython_vm::{Interpreter, PyResult, Settings};
use std::collections::HashMap;
use std::path::Path;



/// Benchmark a Python code snippet using CPython (via PyO3)
fn bench_cpython_code(b: &mut Bencher, source: &str) {
    // Convert the Rust string to a C-compatible string for PyO3
    let c_str_source_head = std::ffi::CString::new(source).unwrap();
    let c_str_source = c_str_source_head.as_c_str();
    // Acquire the Python GIL (Global Interpreter Lock) and run the code
    pyo3::Python::with_gil(|py| {
        b.iter(|| {
            // Compile and execute the Python code as a module
            let module = pyo3::types::PyModule::from_code(py, c_str_source, c"", c"")
                .expect("Error running source");
            // Use black_box to prevent compiler optimizations
            black_box(module);
        })
    })
}

/// Benchmark a Python code snippet using RustPython
fn bench_rustpython_code(b: &mut Bencher, name: &str, source: &str) {
    // NOTE: Take long time.
    // Set up RustPython interpreter settings
    let mut settings = Settings::default();
    settings.path_list.push("Lib/".to_string());
    settings.write_bytecode = false;
    settings.user_site_directory = false;
    // Create an interpreter without the standard library
    Interpreter::without_stdlib(settings).enter(|vm| {
        // Note: bench_cpython is both compiling and executing the code.
        // As such we compile the code in the benchmark loop as well.
        // For each iteration, compile and execute the code
        b.iter(|| {
            // Compile the Python code
            let code = vm.compile(source, Mode::Exec, name.to_owned()).unwrap();
            // Create a new scope (namespace) with built-in functions
            let scope = vm.new_scope_with_builtins();
            // Run the compiled code in the new scope
            let res: PyResult = vm.run_code_obj(code.clone(), scope);
            // Unwrap the result, panicking if there was an error
            vm.unwrap_pyresult(res);
        })
    })
}



/// Main function that sets up and runs all benchmarks
pub fn criterion_benchmark(c: &mut Criterion) {
    // Path to the directory containing benchmark files
    let benchmark_dir = Path::new("./benches/benchmarks/");
    // Read all files in the directory into a HashMap (filename -> file contents)
    let mut benches = benchmark_dir
        .read_dir()
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect::<HashMap<_, _>>();


    /********************************************
    *           benchmark_file_parsing
    *********************************************/
    // Benchmark parsing for each file
    let mut parse_group = c.benchmark_group("parse_to_ast");
    for (name, contents) in &benches {

        // Set the throughput (for reporting) to the number of bytes in the file
        parse_group.throughput(Throughput::Bytes(contents.len() as u64));
        // Benchmark parsing with RustPython's parser
        parse_group.bench_function(BenchmarkId::new("rustpython", name), |b| {
            b.iter(|| ruff_python_parser::parse_module(contents).unwrap())
        });
        // Benchmark parsing with CPython's parser (via PyO3)
        parse_group.bench_function(BenchmarkId::new("cpython", name), |b| {
            use pyo3::types::PyAnyMethods;
            pyo3::Python::with_gil(|py| {
                let builtins =
                    pyo3::types::PyModule::import(py, "builtins").expect("Failed to import builtins");
                let compile = builtins.getattr("compile").expect("no compile in builtins");
                b.iter(|| {
                    let x = compile
                        .call1((contents, name, "exec"))
                        .expect("Failed to parse code");
                    black_box(x);
                })
            })
        });

    }
    parse_group.finish();


    /********************************************
    *           benchmark_pystone
    *********************************************/

    // If there is a PyStone benchmark, run it separately
    if let Some(pystone_contents) = benches.remove("pystone.py") {
        let mut pystone_group = c.benchmark_group("pystone");
        //benchmark_pystone(&mut pystone_group, pystone_contents);
        for idx in (10_000..=30_000).step_by(10_000) {
            // Insert the number of loops into the code
            let code_with_loops = format!("LOOPS = {}\n{}", idx, pystone_contents);
            let code_str = code_with_loops.as_str();
    
            // Set throughput to the number of loops
            pystone_group.throughput(Throughput::Elements(idx as u64));
            // Benchmark with CPython
            pystone_group.bench_function(BenchmarkId::new("cpython", idx), |b| {
                bench_cpython_code(b, code_str)
            });
            // Benchmark with RustPython
            pystone_group.bench_function(BenchmarkId::new("rustpython", idx), |b| {
                bench_rustpython_code(b, "pystone", code_str)
            });
        }
        pystone_group.finish();
    }

    /********************************************
    *           benchmark_execution
    *********************************************/
    // Benchmark execution for each file
    let mut execution_group = c.benchmark_group("execution");
    for (name, contents) in &benches {
        // Register the CPython benchmark
        execution_group.bench_function(BenchmarkId::new("cpython", name), |b| {
            bench_cpython_code(b, contents)
        });
        // Register the RustPython benchmark
        execution_group.bench_function(BenchmarkId::new("rustpython", name), |b| {
            bench_rustpython_code(b, name, contents)
        });
    }
    execution_group.finish();

}

// Register the benchmarks with Criterion
criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);