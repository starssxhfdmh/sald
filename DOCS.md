# Sald Language Reference

Sald is a modern, dynamic, class-based programming language built with Rust. It features async/await, a rich standard library, and a built-in package manager.

---

## Table of Contents

- [Getting Started](#getting-started)
- [Language Basics](#language-basics)
- [Value Types](#value-types)
- [Literals](#literals)
- [Operators](#operators)
- [Control Flow](#control-flow)
- [Functions](#functions)
- [Classes](#classes)
- [Standard Library](#standard-library)
- [Module System](#module-system)
- [CLI Commands](#cli-commands)

---

## Getting Started

```bash
# Build from source
git clone https://github.com/starssxhfdmh/sald.git
cd sald && cargo build --release

# Run a script
sald script.sald

# Start REPL
sald

# Package manager
salad new my-project
salad add spark
salad run
```

---

## Language Basics

### Variable Declaration

```javascript
let x = 5
let y               // null by default
const NAME = "Sald" // immutable
```

### Comments

```javascript
// Single line comment

/* 
  Multi-line
  comment
*/
```

---

## Value Types

### Primitive Types

| Type | Example | Truthy | Falsy |
|------|---------|--------|-------|
| Number | `42`, `3.14` | Non-zero | `0` |
| String | `"hello"` | Non-empty | `""` |
| Boolean | `true`, `false` | `true` | `false` |
| Null | `null` | - | Always falsy |

### Collection Types

| Type | Example | Truthy | Falsy |
|------|---------|--------|-------|
| Array | `[1, 2, 3]` | Non-empty | `[]` |
| Dictionary | `{"a": 1}` | Non-empty | `{}` |

### Internal Types

```
Function, NativeFunction, Class, Instance, 
Future, Namespace, Enum, BoundMethod
```

---

## Literals

### Numbers

```javascript
42          // Integer
3.14        // Float
-100        // Negative
```

### Strings

```javascript
// Normal strings (escape sequences work)
"hello\nworld"
'single quotes'

// Escape sequences
"\n"        // newline
"\t"        // tab
"\\"        // backslash
"\""        // double quote
"\x41"      // hex (A)
"\u0041"    // unicode (A)
"\u{1F600}" // unicode emoji (😀)

// Multiline strings (escape sequences work)
"""
Hello World!
This is a multiline string.
Supports \n escape sequences.
"""

// Format strings (interpolation + escapes)
$"Hello, {name}!"
$"Result: {x + y}"
$"Escaped {{braces}}"

// Format multiline strings (interpolation + escapes)
$"""
Hello {name}!
Supports \n escapes too.
"""

// Raw strings (NO escape processing)
r"C:\path\to\file"      // single-line raw
r'Also works with single quotes'

// Raw multiline strings (NO escape processing)
r"""
This is raw.
No \n processing here.
{curly braces are literal}
"""
```

### Arrays and Dictionaries

```javascript
let arr = [1, 2, 3]
let dict = {"name": "Sald", "version": 1}

// Access
arr[0]          // 1
dict["name"]    // "Sald"

// Array Destructuring
let [a, b, c] = [1, 2, 3]       // a=1, b=2, c=3
let [first, ...rest] = [1,2,3]  // first=1, rest=[2,3]
let [x, , z] = [1, 2, 3]        // x=1, z=3 (skip middle)

// Dict Unpacking (spread)
let base = {"a": 1, "b": 2}
let extended = {**base, "c": 3}  // {"a": 1, "b": 2, "c": 3}
let override = {**base, "a": 10} // {"a": 10, "b": 2}
```

---

## Operators

### Arithmetic
`+` `-` `*` `/` `%`

### Comparison
`==` `!=` `<` `<=` `>` `>=`

### Logical
`&&` `||` `!`

### Bitwise
```javascript
5 & 3   // 1  (AND)
5 | 3   // 7  (OR)
5 ^ 3   // 6  (XOR)
~5      // -6 (NOT)
5 << 2  // 20 (Left Shift)
20 >> 2 // 5  (Right Shift)
```

### Assignment
`=` `+=` `-=` `*=` `/=` `%=`

### Null Coalescing
```javascript
value ?? "default"  // returns left if not null
```

### Ternary
```javascript
condition ? trueValue : falseValue
```

### Spread
```javascript
let args = [1, 2, 3]
func(...args)  // expands to func(1, 2, 3)
```

### Optional Chaining
```javascript
// Safe property/method access on nullable values
let maybeNull = null
maybeNull?.length()   // null (no error)
maybeNull?.toString() // null (no error)

// Works with method calls
let arr = [1, 2, 3]
arr?.first()          // 1
null?.first()         // null
```

### Range Operators
```javascript
// Inclusive range (start..end)
1..5        // [1, 2, 3, 4, 5]

// Exclusive range (start..<end)
1..<5       // [1, 2, 3, 4]

// Use in loops
for i in 0..<10 {
    Console.println(i)
}

// Negative ranges
-3..3       // [-3, -2, -1, 0, 1, 2, 3]
```

---

## Control Flow

### If Statement

```javascript
if condition {
    // body
} else if other {
    // body
} else {
    // body
}
```

### While Loop

```javascript
while condition {
    // body
}
```

### Do-While Loop

```javascript
do {
    // body
} while condition
```

### For-In Loop

```javascript
for item in array {
    Console.println(item)
}

for i in 0..<10 {
    Console.println(i)  // 0..9
}
```

### Switch Expression

```javascript
// Basic switch
let result = switch value {
    1 -> "one"
    2, 3 -> "two or three"
    default -> "other"
}

// Range patterns
let grade = switch score {
    90..100 -> "A"      // inclusive range
    80..<90 -> "B"      // exclusive range
    70..<80 -> "C"
    default -> "F"
}

// Guard expressions (if)
let category = switch n {
    x if x < 0 -> "negative"
    x if x == 0 -> "zero"
    x if x < 10 -> "small"
    x -> "large"
}

// Array destructuring
let desc = switch arr {
    [] -> "empty"
    [x] -> $"single: {x}"
    [a, b] -> $"pair: {a}, {b}"
    [head, ...tail] -> $"head: {head}, rest: {tail}"
}

// Dict destructuring
let msg = switch event {
    {"type": "click", "target": t} -> $"clicked on {t}"
    {"type": "load"} -> "page loaded"
    _ -> "unknown event"
}
```

### Break and Continue

```javascript
while true {
    if done { break }
    if skip { continue }
}
```

---

## Functions

### Function Declaration

```javascript
fun greet(name) {
    Console.println($"Hello, {name}!")
}

// Default parameters
fun connect(host, port = 8080) {
    // ...
}

// Variadic parameters (must be last)
fun sum(...numbers) {
    let total = 0
    for n in numbers {
        total += n
    }
    return total
}
```

### Lambda (Anonymous Functions)

```javascript
|x| x * 2                    // expression body
|a, b| { return a + b }      // block body
async |x| await fetch(x)     // async lambda
```

### Named Arguments

```javascript
fun createUser(name, age, role = "user") { ... }

// Call with named arguments (order doesn't matter)
createUser(age: 25, name: "John")
createUser(role: "admin", name: "Jane", age: 30)
```

### Async Functions

```javascript
async fun fetchData(url) {
    let response = await Http.get(url)
    return Json.parse(response)
}

// Try-catch with async
try {
    let data = await fetchData("/api/users")
} catch (e) {
    Console.println($"Error: {e}")
}
```

---

## Classes

### Basic Class

```javascript
class Animal {
    // Constructor (receives self implicitly)
    fun init(self, name) {
        self.name = name
    }
    
    // Instance method
    fun speak(self) {
        Console.println($"{self.name} makes a sound")
    }
    
    // Static method (no self parameter)
    fun create(self, name) {
        return Animal(name)
    }
}

let dog = Animal("Rex")
dog.speak()

let cat = Animal.create("Whiskers")
```

> **Note:** There is NO `static` keyword. Methods without `self` are automatically static.

### Inheritance

```javascript
class Dog extends Animal {
    fun speak(self) {
        Console.println($"{self.name} barks!")
    }
    
    fun speakTwice(self) {
        super.speak()  // call parent
        self.speak()
    }
}
```

### Namespace

```javascript
namespace Utils {
    const VERSION = "1.0"
    
    fun helper() {
        return "helping"
    }
}

Utils.VERSION   // "1.0"
Utils.helper()  // "helping"
```

### Enum

```javascript
enum Status {
    Pending,
    Active,
    Completed
}

let s = Status.Active
```

### Interface

Interfaces define contracts that classes must implement:

```javascript
// Define an interface
interface Printable {
    fun print(self)
}

interface Comparable {
    fun compare(self, other)
    fun equals(self, other)
}

// Implement interfaces
class Document implements Printable {
    fun init(self, content) {
        self.content = content
    }
    
    fun print(self) {
        Console.println(self.content)
    }
}

// Multiple interfaces + inheritance
class Article extends Document implements Printable, Comparable {
    fun compare(self, other) {
        return self.content.length() - other.content.length()
    }
    
    fun equals(self, other) {
        return self.content == other.content
    }
}
```

Interface validation happens at **compile time** - if a class doesn't implement all required methods, compilation fails:

```javascript
interface Drawable {
    fun draw(self, x, y)
}

class Circle implements Drawable {
    // Error: Class 'Circle' does not implement method 'draw' 
    //        required by interface 'Drawable'
}
```

### Decorators

Decorators are annotations that modify functions or classes:

```javascript
// Simple decorator
@test
fun test_addition() {
    Test.assert_eq(1 + 1, 2)
}

// Decorator with arguments  
@test("Addition should work correctly")
fun test_math() {
    Test.assert(2 + 2 == 4)
}

// Multiple decorators
@deprecated
@test
fun test_old_api() {
    // ...
}
```

### Testing Framework

Built-in testing with the `@test` decorator and `Test` class:

```javascript
// tests/math_test.sald

@test
fun test_addition() {
    Test.assert_eq(1 + 1, 2)
    Test.assert_eq(10 + 5, 15)
}

@test
fun test_strings() {
    let s = "Hello"
    Test.assert_eq(s.length(), 5)
    Test.assert_ne(s, "World")
}

@test
fun test_boolean() {
    Test.assert(true)
    Test.assert(5 > 3)
}

@test
fun test_failure_example() {
    Test.fail("This test always fails")
}
```

**Test Assertions:**

| Method | Description |
|--------|-------------|
| `Test.assert(condition, ?message)` | Fails if condition is falsy |
| `Test.assert_eq(actual, expected, ?message)` | Fails if actual != expected |
| `Test.assert_ne(actual, expected, ?message)` | Fails if actual == expected |
| `Test.fail(?message)` | Always fails |

**Running Tests:**

```bash
# Run all tests in a file
sald --test tests/math_test.sald

# Filter tests by name
sald --test tests/math_test.sald --filter addition

# Short form
sald -t tests/math_test.sald -f add
```

**Output Format (Rust-style):**

```
running 4 tests
test test_addition ... ok
test test_strings ... ok (0.12ms)
test test_boolean ... ok
test test_failure_example ... FAILED

failures:

---- test_failure_example ----
AssertionError: This test always fails

failures:
    test_failure_example

test result: FAILED. 3 passed; 1 failed; finished in 0.01s
```

---

## Standard Library

### String

Instance methods (called on string values):

```javascript
let s = "Hello World"

s.length()              // 11
s.upper()               // "HELLO WORLD"
s.lower()               // "hello world"
s.trim()                // remove whitespace
s.trimStart()           // remove leading whitespace
s.trimEnd()             // remove trailing whitespace
s.contains("World")     // true
s.includes("World")     // true (alias for contains)
s.startsWith("Hello")   // true
s.endsWith("World")     // true
s.indexOf("o")          // 4 (first occurrence)
s.indexOf("o", 5)       // 7 (from index 5)
s.lastIndexOf("o")      // 7 (last occurrence)
s.replace("World", "Sald")  // "Hello Sald"
s.replaceAll("o", "0")  // "Hell0 W0rld"
s.split(" ")            // ["Hello", "World"]
s.substring(0, 5)       // "Hello"
s.slice(-5)             // "World" (negative index)
s.charAt(0)             // "H"
s.isDigit()             // false
s.padStart(15, "*")     // "****Hello World"
s.padEnd(15, "*")       // "Hello World****"
s.repeat(2)             // "Hello WorldHello World"
s.toString()            // "Hello World"
```

Static methods:

```javascript
String.fromCharCode(65)     // "A"
String.charCodeAt("A", 0)   // 65
```

---

### Number

Instance methods:

```javascript
let n = -3.14159

n.abs()         // 3.14159
n.floor()       // -4
n.ceil()        // -3
n.round()       // -3
n.toFixed(2)    // "-3.14"
n.toString()    // "-3.14159"
```

---

### Boolean

```javascript
true.toString()   // "true"
false.toString()  // "false"
```

---

### Array

Instance methods:

```javascript
let arr = [1, 2, 3]

// Properties
arr.length()            // 3

// Mutating methods
arr.push(4)             // add to end, returns new length
arr.pop()               // remove from end, returns removed
arr.shift()             // remove from start
arr.unshift(0)          // add to start
arr.removeAt(1)         // remove at index
arr.splice(1, 1, 10)    // remove 1 at index 1, insert 10
arr.clear()             // remove all
arr.reverse()           // reverse in-place

// Non-mutating methods
arr.first()             // first element or null
arr.last()              // last element or null
arr.get(0)              // element at index
arr.set(0, 10)          // set element, returns array
arr.slice(1, 3)         // sub-array (negative indices work)
arr.concat([4, 5])      // new merged array
arr.join(", ")          // "1, 2, 3"
arr.contains(2)         // true
arr.indexOf(2)          // 1 (or -1 if not found)
arr.isEmpty()           // false
arr.keys()              // [0, 1, 2] (indices)
arr.toString()          // "[1, 2, 3]"

// Higher-order methods
arr.map(|x| x * 2)           // [2, 4, 6]
arr.filter(|x| x > 1)        // [2, 3]
arr.reduce(|a, b| a + b, 0)  // 6
arr.forEach(|x| Console.println(x))
arr.find(|x| x > 1)          // 2
arr.findIndex(|x| x > 1)     // 1
arr.some(|x| x > 2)          // true
arr.every(|x| x > 0)         // true
arr.sort()                   // sort in-place (string comparison)
arr.sort(|a, b| a - b)       // sort with comparator
arr.flatMap(|x| [x, x*2])    // flatten mapped arrays

// New array methods
arr.at(-1)               // 3 (last element, negative index)
arr.fill(0)              // [0, 0, 0] (fill all with 0)
arr.fill(0, 1, 2)        // [1, 0, 3] (fill range)
arr.flat()               // flatten nested arrays 1 level
arr.flat(2)              // flatten 2 levels deep
arr.toReversed()         // new reversed array (non-mutating)
arr.toSorted()           // new sorted array (non-mutating)
arr.toSorted(|a,b| b-a)  // with comparator
```

---

### Dict (Dictionary)

```javascript
let dict = {"name": "John", "age": 25}

// Methods
dict.length()           // 2
dict.keys()             // ["name", "age"]
dict.values()           // ["John", 25]
dict.entries()          // [["name", "John"], ["age", 25]]
dict.has("name")        // true
dict.get("name")        // "John"
dict.get("role", "user")  // "user" (default)
dict.set("role", "admin")
dict.remove("role")     // returns removed value
dict.clear()
dict.isEmpty()          // true
dict.toString()         // '{"name": "John", "age": 25}'

// Constructor
let copy = Dict(dict)   // copy dictionary
```

---

### Console

```javascript
Console.print("no newline")
Console.println("with newline")
Console.input("Enter name: ")  // read line from stdin
Console.clear()                // clear terminal
```

---

### Math

Constants:

```javascript
Math.PI           // 3.14159...
Math.E            // 2.71828...
Math.INFINITY     // +∞
Math.NEG_INFINITY // -∞
Math.NAN          // Not a Number
```

Methods:

```javascript
Math.abs(-5)      // 5
Math.floor(3.7)   // 3
Math.ceil(3.2)    // 4
Math.round(3.5)   // 4
Math.sqrt(16)     // 4
Math.pow(2, 8)    // 256
Math.sin(0)       // 0
Math.cos(0)       // 1
Math.tan(0)       // 0
Math.asin(0)      // 0
Math.acos(1)      // 0
Math.atan(0)      // 0
Math.log(Math.E)  // 1 (natural log)
Math.log10(100)   // 2
Math.exp(1)       // Math.E
Math.random()     // 0.0 to 1.0
Math.min(1, 2, 3) // 1
Math.max(1, 2, 3) // 3
```

---

### Date

```javascript
Date.now()        // "2024-01-15 14:30:45"
Date.timestamp()  // 1705329045 (Unix seconds)
Date.year()       // 2024
Date.month()      // 1
Date.day()        // 15
Date.hour()       // 14
Date.minute()     // 30
Date.second()     // 45
Date.format("YYYY/MM/DD HH:mm:ss")
```

---

### Timer

```javascript
await Timer.sleep(1000)  // async sleep (ms)
Timer.now()              // milliseconds since epoch
Timer.millis()           // alias for now()
```

---

### Type

```javascript
Type.of(value)       // "String", "Number", etc.
Type.isString(val)   // true/false
Type.isNumber(val)
Type.isBoolean(val)
Type.isNull(val)
Type.isArray(val)
Type.isDict(val)
Type.isFunction(val)
Type.isClass(val)
Type.isInstance(val)
```

---

### Json

```javascript
let obj = Json.parse('{"name": "John"}')
let str = Json.stringify(obj)
let pretty = Json.stringify(obj, 2)  // 2-space indent
```

---

### Regex

```javascript
// Create regex with optional flags
let r = Regex.new("\\d+")           // digits
let r2 = Regex.new("hello", "i")    // case-insensitive

// Flags: i (case-insensitive), m (multiline), s (dot matches newline)
```

Instance methods:

```javascript
let r = Regex.new("(\\w+)@(\\w+)")

r.test("test@example")              // true
r.match("test@example")             // ["test@example", "test", "example"]
r.matchAll("a@b and c@d")           // [["a@b", "a", "b"], ["c@d", "c", "d"]]
r.replace("test@a", "$1@new")       // "test@new" (first match)
r.replaceAll("a@b c@d", "X")        // "X X"
r.split("a1b2c")                    // ["a", "b", "c"] (with Regex.new("\\d"))
r.pattern()                         // "(\\w+)@(\\w+)"
r.flags()                           // "" or "i", "m", "s", etc.
```

---

### Crypto

```javascript
// Hashing (sha256, sha512, md5, sha1)
Crypto.hash("sha256", "hello")      // "2cf24dba5fb0a30e..."
Crypto.hash("md5", "hello")         // "5d41402abc4b2a76..."

// HMAC signing (sha256, sha512)
Crypto.hmac("sha256", "secret", "message")

// UUID v4
Crypto.uuid()                       // "550e8400-e29b-41d4-..."

// Random
Crypto.randomBytes(16)              // [23, 45, 128, ...] (16 bytes)
Crypto.randomInt(1, 100)            // random int between 1-100

// Base64
Crypto.base64Encode("hello")        // "aGVsbG8="
Crypto.base64Decode("aGVsbG8=")     // "hello"
```

---

### Channel

Go-style channels for async communication:

```javascript
let ch = Channel()        // buffered (default 16)
let ch = Channel(100)     // custom buffer size

// Async send/receive
await ch.send(value)
let msg = await ch.receive()

// Non-blocking
let msg = ch.tryReceive() // value or null

// Close
ch.close()
ch.isClosed()             // true/false
```

Example producer/consumer:

```javascript
let ch = Channel()

async fun producer() {
    for i in 0..5 {
        await ch.send(i)
    }
    ch.close()
}

async fun consumer() {
    while !ch.isClosed() {
        let msg = await ch.receive()
        if msg != null {
            Console.println($"Got: {msg}")
        }
    }
}

await Promise.all([producer(), consumer()])
```

---

### Promise

Parallel async execution:

```javascript
// Wait for all futures
let results = await Promise.all([
    fetchUser(1),
    fetchUser(2),
    fetchUser(3)
])
// results = [user1, user2, user3]

// First to complete wins
let fastest = await Promise.race([
    Timer.sleep(100),
    Timer.sleep(50)   // wins
])

// Create resolved/rejected futures
let resolved = Promise.resolve(42)
let rejected = Promise.reject("error")
```

---

### File (Async)

```javascript
// All file operations are async
let content = await File.read("path/to/file.txt")
await File.write("file.txt", "content")
await File.append("file.txt", "more content")
await File.delete("file.txt")
await File.copy("src.txt", "dst.txt")  // returns bytes copied
await File.rename("old.txt", "new.txt")
await File.mkdir("new-folder")  // recursive
await File.exists("file.txt")   // true/false
await File.isFile("path")
await File.isDir("path")
await File.size("file.txt")     // bytes
let files = await File.readDir("folder")

// Sync path utilities
File.join("a", "b", "c")   // "a/b/c"
File.dirname("/a/b/c")     // "/a/b"
File.basename("/a/b/c.txt") // "c.txt"
File.ext("/a/b/c.txt")     // ".txt"
```

---

### Path

```javascript
Path.join("a", "b", "c")    // "a/b/c"
Path.dirname("/a/b/c")      // "/a/b"
Path.basename("/a/b/c.txt") // "c.txt"
Path.extname("/a/b/c.txt")  // ".txt"
Path.isAbsolute("/a/b")     // true
Path.exists("path")         // true/false
Path.normalize("a//b/../c") // "a/c"
```

---

### Process

```javascript
Process.args()        // command line arguments
Process.env("HOME")   // environment variable
Process.cwd()         // current working directory
Process.chdir("/tmp") // change directory
Process.exec("ls -la") // run shell command
Process.exit(0)       // exit with code
```

---

### System

Basic info:

```javascript
System.os()           // "linux", "windows", "macos"
System.arch()         // "x86_64", "aarch64"
System.family()       // "unix", "windows"
System.cpus()         // number of logical CPUs
System.hostname()
System.osVersion()    // "Windows 11 (22H2)"
System.kernelVersion()
```

Memory:

```javascript
System.totalMemory()  // bytes
System.usedMemory()
System.freeMemory()
System.totalSwap()
System.usedSwap()
```

CPU:

```javascript
System.cpuName()      // "Intel Core i7-..."
System.cpuUsage()     // 0-100 percentage
```

Other:

```javascript
System.uptime()       // seconds
System.bootTime()     // Unix timestamp
System.info()         // all stats as dictionary

System.getenv("HOME")
System.setenv("VAR", "value")
System.envs()         // all env vars
```

---

### Http

Client (async):

```javascript
let body = await Http.get("https://api.example.com")
let result = await Http.post(url, "{\"data\": 1}")
await Http.put(url, body)
await Http.delete(url)
```

Server:

```javascript
let server = Http.Server()

server.get("/", |req| {
    return { body: "Hello World" }
})

server.get("/users/:id", |req| {
    let id = req.params["id"]
    return { 
        status: 200,
        headers: { "Content-Type": "application/json" },
        body: Json.stringify({ id: id })
    }
})

server.post("/api/data", |req| {
    let data = Json.parse(req.body)
    return { body: "OK" }
})

// Available methods: get, post, put, delete, patch, options, head, all

server.routes()  // list registered routes
server.listen(8080)
```

---

### Ffi (Foreign Function Interface)

```javascript
// Load native library
let lib = Ffi.load("./path/to/library")  // .dll/.so/.dylib

// Call function
let result = lib.call("add", 10, 20)

// Read C string from pointer
let str = Ffi.readString(ptr)

// Callbacks (Sald -> C)
let cb = Ffi.callback(|a, b| a + b)
lib.call("register_callback", cb.id, cb.invoker)
Ffi.removeCallback(cb.id)

// Cleanup
lib.close()
lib.path()  // get library path
```

---

## Module System

### Project Structure

```
my-project/
├── salad.json      # Project manifest
├── main.sald       # Entry point
└── sald_modules/   # Dependencies
```

### salad.json

```json
{
  "name": "my-project",
  "version": "1.0.0",
  "description": "My Sald project",
  "author": "username",
  "license": "MIT",
  "main": "main.sald",
  "modules": {
    "spark": "1.0.0",
    "uuid": "1.0.0"
  }
}
```

### Import

```javascript
import "utils.sald"              // global import
import "utils.sald" as Utils     // aliased import
import "spark"                   // module from sald_modules/
```

---

## CLI Commands

### sald (Runtime)

```bash
sald                    # Start REPL
sald script.sald        # Run script
sald -c script.sald     # Compile to bytecode
```

REPL commands:

```
>>> .help    # Show help
>>> .clear   # Clear screen
```

### salad (Package Manager)

```bash
salad new project-name  # Create new project
salad init              # Initialize in current dir
salad run               # Run main script
salad run -- arg1 arg2  # Run with arguments
salad check             # Verify modules
salad install           # Install all modules
salad add spark         # Add package (latest)
salad add spark@1.0.0   # Add specific version
salad remove spark      # Remove package
salad prune             # Remove unused modules
salad login             # Login to registry
salad logout            # Logout
salad whoami            # Show current user
salad publish           # Publish package
```

---

## Keywords

```
let, const, if, else, while, do, for, in,
fun, return, class, extends, self, super,
try, catch, throw, async, await,
switch, default, break, continue,
import, as, namespace, enum,
true, false, null
```

---

## License

MIT License
