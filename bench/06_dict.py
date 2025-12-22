# Benchmark: Dictionary Operations
# Tests: Dict creation, access, modification, iteration

import time

start = time.time()

# Create large dictionary
d = {}
for i in range(10000):
    d[f"key_{i}"] = i * 2

# Access operations
sum_val = 0
for i in range(10000):
    sum_val += d[f"key_{i}"]

# Check existence
exists_count = 0
for i in range(10000):
    if f"key_{i}" in d:
        exists_count += 1

# Get with default
with_default = 0
for i in range(100):
    with_default += d.get(f"missing_{i}", 0)

# Nested dictionaries
nested = {}
for i in range(1000):
    nested[f"item_{i}"] = {
        "id": i,
        "name": f"Item {i}",
        "data": {
            "value": i * 10,
            "active": i % 2 == 0
        }
    }

# Access nested values
nested_sum = 0
for key in nested.keys():
    nested_sum += nested[key]["data"]["value"]

elapsed = (time.time() - start) * 1000

print(f"Dict size: {len(d)}")
print(f"Sum: {sum_val}")
print(f"Exists count: {exists_count}")
print(f"Nested sum: {nested_sum}")
print(f"Time: {elapsed:.2f}ms")
