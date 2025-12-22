# Benchmark: Iterative Sum
# Tests: Loop performance, basic arithmetic

import time

def iterative_sum(n):
    sum = 0
    i = 0
    while i < n:
        sum += i
        i += 1
    return sum

start = time.time()
result = iterative_sum(1000000)
elapsed = (time.time() - start) * 1000

print(f"Sum(0..1000000) = {result}")
print(f"Time: {elapsed:.2f}ms")
