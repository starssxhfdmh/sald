# Benchmark: Concurrent Queue Communication  
# Tests: Thread-safe queue for Python comparison

import threading
import queue
import time

MESSAGES = 10000
PRODUCERS = 4

def producer(q, producer_id, count):
    """Producer that sends messages to queue"""
    for i in range(count):
        q.put(producer_id * 10000 + i)

def consumer(q, expected):
    """Consumer that receives expected number of messages"""
    received = 0
    while received < expected:
        try:
            msg = q.get(timeout=1.0)
            received += 1
        except queue.Empty:
            pass
    return received

# Run benchmark
q = queue.Queue(maxsize=100)

start = time.perf_counter()

per_producer = MESSAGES // PRODUCERS
threads = []

# Start producer threads
for i in range(PRODUCERS):
    t = threading.Thread(target=producer, args=(q, i, per_producer))
    t.start()
    threads.append(t)

# Start consumer thread
consumer_result = [0]
def consumer_wrapper():
    consumer_result[0] = consumer(q, MESSAGES)
    
consumer_thread = threading.Thread(target=consumer_wrapper)
consumer_thread.start()
threads.append(consumer_thread)

# Wait for all threads
for t in threads:
    t.join()

elapsed = (time.perf_counter() - start) * 1000

print(f"Sent {MESSAGES} messages via {PRODUCERS} producers")
print(f"Time: {elapsed:.2f}ms")
print(f"Throughput: {MESSAGES / elapsed * 1000:.0f} msg/sec")
