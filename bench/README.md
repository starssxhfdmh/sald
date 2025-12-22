# Sald vs Python Benchmarks

Benchmark comparisons between Sald and Python.

## Running Benchmarks

### Run All (Recommended)

```bash
python bench/run_all.py
```

This will:
1. Run all Sald benchmarks
2. Run all Python benchmarks  
3. Display results table
4. Generate `benchmark_results.png` comparison graph

**Graph fallback**: matplotlib → Pillow → ASCII chart

### Run Individual

```bash
# Sald
sald bench/01_fibonacci.sald

# Python
python bench/01_fibonacci.py
```

## Benchmarks

| # | File | Tests |
|---|------|-------|
| 01 | `fibonacci` | Recursive function calls |
| 02 | `iterative` | While loop, arithmetic |
| 03 | `array_ops` | map, filter, reduce |
| 04 | `string` | Concatenation, interpolation |
| 05 | `class` | OOP, object creation |
| 06 | `dict` | Dictionary CRUD |
| 07 | `higher_order` | Composition, currying |
| 08 | `primes` | Sieve algorithm |

## Requirements

For graph generation (optional):
- `matplotlib`