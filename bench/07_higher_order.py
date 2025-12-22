# Benchmark: Higher-Order Functions
# Tests: Function composition, closures, callbacks

import time
from functools import reduce

def compose(*fns):
    def composed(x):
        result = x
        for fn in reversed(fns):
            result = fn(result)
        return result
    return composed

def curry(fn):
    def curried(a):
        def inner(b):
            return fn(a, b)
        return inner
    return curried

def memoize(fn):
    cache = {}
    def memoized(n):
        key = str(n)
        if key in cache:
            return cache[key]
        result = fn(n)
        cache[key] = result
        return result
    return memoized

start = time.time()

# Compose functions
add_one = lambda x: x + 1
double = lambda x: x * 2
square = lambda x: x * x

composed = compose(add_one, double, square)

sum_val = 0
for i in range(10000):
    sum_val += composed(i)

# Currying
add = lambda a, b: a + b
curried_add = curry(add)
add5 = curried_add(5)

curried_sum = 0
for i in range(10000):
    curried_sum += add5(i)

# Memoized fibonacci (need to define before memoize for recursion)
memo_cache = {}
def memo_fib(n):
    if n in memo_cache:
        return memo_cache[n]
    if n <= 1:
        result = n
    else:
        result = memo_fib(n - 1) + memo_fib(n - 2)
    memo_cache[n] = result
    return result

fib_result = memo_fib(30)

# Callback chains
def process(data, *callbacks):
    result = data
    for cb in callbacks:
        result = cb(result)
    return result

processed = process(
    10,
    lambda x: x + 1,
    lambda x: x * 2,
    lambda x: x - 5,
    lambda x: x * x
)

elapsed = (time.time() - start) * 1000

print(f"Composed sum: {sum_val}")
print(f"Curried sum: {curried_sum}")
print(f"Memoized fib(30): {fib_result}")
print(f"Processed: {processed}")
print(f"Time: {elapsed:.2f}ms")
