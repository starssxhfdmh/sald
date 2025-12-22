# Benchmark: Prime Sieve (Sieve of Eratosthenes)
# Tests: Combined array operations, loops, conditionals

import time

def sieve(limit):
    # Initialize array with true values
    is_prime = [True] * (limit + 1)
    
    is_prime[0] = False
    is_prime[1] = False
    
    # Sieve
    i = 2
    while i * i <= limit:
        if is_prime[i]:
            j = i * i
            while j <= limit:
                is_prime[j] = False
                j += i
        i += 1
    
    # Collect primes
    primes = []
    for i in range(limit + 1):
        if is_prime[i]:
            primes.append(i)
    
    return primes

def is_prime_check(n):
    if n < 2:
        return False
    if n == 2:
        return True
    if n % 2 == 0:
        return False
    
    i = 3
    while i * i <= n:
        if n % i == 0:
            return False
        i += 2
    return True

start = time.time()

# Sieve for primes up to 100000
primes = sieve(100000)

# Sum of primes
sum_val = sum(primes)

# Individual prime checks
check_count = 0
for i in range(10000):
    if is_prime_check(i):
        check_count += 1

# Find nth prime
def nth_prime(n):
    count = 0
    num = 2
    while count < n:
        if is_prime_check(num):
            count += 1
            if count == n:
                return num
        num += 1
    return num

p1000 = nth_prime(1000)

elapsed = (time.time() - start) * 1000

print(f"Primes found (sieve): {len(primes)}")
print(f"Sum of primes: {sum_val}")
print(f"Primes found (check): {check_count}")
print(f"1000th prime: {p1000}")
print(f"Time: {elapsed:.2f}ms")
