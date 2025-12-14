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

// Format strings (interpolation)
$"Hello, {name}!"
$"Result: {x + y}"
$"Escaped {{braces}}"

// Raw strings (multiline, no escapes)
"""
Raw string content.
No \n processing.
"""

// Raw format strings
$"""Hello {name}, no \n escapes."""
```

### Arrays and Dictionaries

```javascript
let arr = [1, 2, 3]
let dict = {"name": "Sald", "version": 1}

// Access
arr[0]          // 1
dict["name"]    // "Sald"
dict.name       // "Sald"
```

---

## Operators

### Arithmetic
`+` `-` `*` `/` `%`

### Comparison
`==` `!=` `<` `<=` `>` `>=`

### Logical
`&&` `||` `!`

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

for i in Array.range(10) {
    Console.println(i)  // 0..9
}
```

### Switch Expression

```javascript
let result = switch value {
    1 -> "one"
    2, 3 -> "two or three"
    default -> "other"
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
s.contains("World")     // true
s.startsWith("Hello")   // true
s.endsWith("World")     // true
s.indexOf("o")          // 4 (first occurrence)
s.indexOf("o", 5)       // 7 (from index 5)
s.replace("World", "Sald")  // "Hello Sald"
s.split(" ")            // ["Hello", "World"]
s.substring(0, 5)       // "Hello"
s.slice(-5)             // "World" (negative index)
s.charAt(0)             // "H"
s.isDigit()             // false
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

Static methods:

```javascript
Array.range(5)          // [0, 1, 2, 3, 4]
Array.range(2, 5)       // [2, 3, 4]
Array.range(0, 10, 2)   // [0, 2, 4, 6, 8]
```

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
