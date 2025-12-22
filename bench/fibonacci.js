// Fibonacci benchmark for Node.js

// Recursive Fibonacci
function fib(n) {
  if (n <= 1) return n;
  return fib(n - 1) + fib(n - 2);
}

const n = parseInt(process.argv[2]) || 35;
console.log(`Calculating fib(${n})...`);

const start = performance.now();
const result = fib(n);
const end = performance.now();

console.log(`fib(${n}) = ${result}`);
console.log(`Time: ${(end - start).toFixed(2)}ms`);
