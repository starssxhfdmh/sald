# Benchmark: String Manipulation
# Tests: String concatenation, methods, interpolation

import time

start = time.time()

result = ""
for i in range(10000):
    result = result + "a"

# String methods
upper = result.upper()
lower = upper.lower()
length = len(lower)

# String interpolation
messages = []
for i in range(1000):
    messages.append(f"Item {i}: value={i * 2}")

# String operations
sample = "Hello World Hello World Hello World"
replaced = sample.replace("Hello", "Hi")
parts = sample.split(" ")
joined = "-".join(parts)

elapsed = (time.time() - start) * 1000

print(f"String length: {length}")
print(f"Messages count: {len(messages)}")
print(f"Replaced: {replaced}")
print(f"Joined: {joined}")
print(f"Time: {elapsed:.2f}ms")
