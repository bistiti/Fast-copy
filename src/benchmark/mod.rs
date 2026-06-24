// Benchmark module: performs real disk I/O tests to determine the optimal
// size threshold for switching between buffered and unbuffered copy modes.
//
// The benchmark writes test files of various sizes on the DESTINATION volume
// and reads from the SOURCE volume, measuring throughput in both modes.
// The crossover point where unbuffered I/O becomes faster than buffered I/O
// becomes the threshold.
//
// Results are cached per volume serial number to avoid re-running on every launch.

pub mod runner;

pub use runner::{BenchmarkResult, BenchmarkStatus, DiskBenchmark};
