# Benchmark: Recursive Fibonacci
# Tests: Recursion overhead, function call performance

import time

def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)

start = time.time()
result = fib(30)
elapsed = (time.time() - start) * 1000

print(f"Fibonacci(30) = {result}")
print(f"Time: {elapsed:.2f}ms")
