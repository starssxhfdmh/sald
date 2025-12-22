# Benchmark: Array Operations
# Tests: map, filter, reduce performance

import time
from functools import reduce

def create_array(n):
    return list(range(n))

start = time.time()

# Create array of 100000 elements
arr = create_array(100000)

# Map: square each element
squared = list(map(lambda x: x * x, arr))

# Filter: keep only even numbers
evens = list(filter(lambda x: x % 2 == 0, squared))

# Reduce: sum all
total = reduce(lambda a, b: a + b, evens, 0)

elapsed = (time.time() - start) * 1000

print(f"Total: {total}")
print(f"Time: {elapsed:.2f}ms")
