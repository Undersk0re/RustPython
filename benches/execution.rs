use criterion::{
    BenchmarkId, Criterion, Throughput, black_box, criterion_group,
    criterion_main, PlotConfiguration, AxisScale
};


use::criterion::BenchmarkGroup


use std::collections::HashMap;
use std::path::Path;
use pyo3::types::PyAnyMethods;

// Constants for benchmark modes
const MODE_CPYTHON: &str = "cpython";
const MODE_RUSTPYTHON: &str = "rustpython";
const PATH_OUT_RESULT_BENCH:&str = "./benches/benchmarks/";
const TEST_NAMES: [&str; 3] = [
    "parse_to_ast",
    "pystone",
    "execution"
];



struct BenchmarkLocalGroup {
    benches: HashMap<String, String>,
    mode: &'static str,
}

impl BenchmarkLocalGroup {
    fn new(benchmark_dir: &Path, mode: &'static str ) -> Self {
        let benches = benchmark_dir
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

        BenchmarkLocalGroup {
            benches,
            mode,
        }
    }


    fn run_parse_benchmarks(&self, c: &mut Criterion) {
        let mut parse_group = c.benchmark_group(TEST_NAMES[0]);
        
        parse_group.plot_config(PlotConfiguration::default()
            .summary_scale(AxisScale::Logarithmic) );
            
        for (name, contents) in &self.benches {
            parse_group.throughput(Throughput::Bytes(contents.len() as u64));
            
            if self.mode == MODE_RUSTPYTHON {
                parse_group.bench_with_input(
                    BenchmarkId::new(MODE_RUSTPYTHON, name),
                    contents,
                    |b, contents| {
                        b.iter(|| ruff_python_parser::parse_module(contents).unwrap())
                    }
                );
            }
            
            if self.mode == MODE_CPYTHON {
                parse_group.bench_with_input(
                    BenchmarkId::new(MODE_CPYTHON, name),
                    contents,
                    |b, contents| {
                        pyo3::Python::with_gil(|py| {
                            let builtins = pyo3::types::PyModule::import(py, "builtins")
                                .expect("Failed to import builtins");
                            let compile = builtins.getattr("compile")
                                .expect("no compile in builtins");
                            b.iter(|| {
                                let x = compile
                                    .call1((contents, name, "exec"))
                                    .expect("Failed to parse code");
                                black_box(x);
                            })
                        })
                    }
                );
            }
        }
        parse_group.finish();
    }



    fn run_all_benchmarks(&self, c: &mut Criterion) {
        self.run_parse_benchmarks(c);
    }
}

fn criterion_benchmark_c(c: &mut Criterion) {
    let benchmark_c = BenchmarkLocalGroup::new(Path::new(PATH_OUT_RESULT_BENCH), MODE_CPYTHON);
    benchmark_c.run_all_benchmarks(c);
}

fn criterion_benchmark_r(c: &mut Criterion) {
    let benchmark_r = BenchmarkLocalGroup::new(Path::new(PATH_OUT_RESULT_BENCH), MODE_RUSTPYTHON);
    benchmark_r.run_all_benchmarks(c);
}




criterion_group!(
    name = benches;
    config = Criterion::default();
    targets = criterion_benchmark_c,criterion_benchmark_r
);




criterion_main!(benches);