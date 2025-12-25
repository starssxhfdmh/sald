# Benchmark: Parallel Processing
# Tests: Multi-threaded performance using ProcessPoolExecutor

import concurrent.futures
import time

def compute(n):
    """Heavy computation function"""
    total = 0
    for i in range(n):
        total += i
    return total

def run_sequential(tasks, workload):
    """Sequential benchmark"""
    results = []
    for _ in range(tasks):
        results.append(compute(workload))
    return results

def run_parallel(tasks, workload):
    """Parallel benchmark using ProcessPoolExecutor"""
    with concurrent.futures.ThreadPoolExecutor(max_workers=tasks) as executor:
        futures = [executor.submit(compute, workload) for _ in range(tasks)]
        results = [f.result() for f in futures]
    return results

TASKS = 8
WORKLOAD = 100000

# Run sequential
seq_start = time.perf_counter()
seq_results = run_sequential(TASKS, WORKLOAD)
seq_time = (time.perf_counter() - seq_start) * 1000

print(f"Sequential: {seq_time:.2f}ms")

# Run parallel
par_start = time.perf_counter()
par_results = run_parallel(TASKS, WORKLOAD)
par_time = (time.perf_counter() - par_start) * 1000

print(f"Parallel: {par_time:.2f}ms")
print(f"Speedup: {seq_time / par_time:.2f}x")

# For benchmark comparison output
print(f"Time: {par_time:.2f}ms")
