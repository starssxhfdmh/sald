# Sald Programming Language

Sald is a modern, dynamic, class-based programming language built with Rust. It is designed to be fast, expressive, and feature-rich, providing a powerful toolkit for building applications ranging from scripts to web servers.

![Sald Logo](docs/logo.png) <!-- Conceptual logic for logo -->

## Key Features

- **Class-Based OOP**: Familiar object-oriented class syntax with inheritance (`extends`), constructors (`init`), and `super` calls.
- **Async/Await**: First-class support for asynchronous programming, powered by the Tokio runtime.
- **Built-in Standard Library**: Extensive suite of built-in classes including `File`, `Http`, `System`, `Process`, `Json`, `Ffi`, and more.
- **Package Management**: Includes `salad`, a built-in package manager to initialize projects and manage dependencies.
- **Language Server**: Built-in LSP support (`sald-lsp`) for a great editor experience.
- **REPL**: Interactive shell for quick experimentation.
- **FFI**: Foreign Function Interface to load and call native C libraries dynamically.

## Installation

To build Sald from source, you need a Rust environment (latest stable).

```bash
# Clone the repository
git clone https://github.com/starssxhfdmh/sald.git
cd sald

# Build release version
cargo build --release
```

The binaries will be available in `target/release/`:
- `sald`: The interpreter and CLI.
- `salad`: The package manager.
- `sald-lsp`: The language server.

## Usage

### Running a Script

```bash
./target/release/sald path/to/script.sald
```

### REPL

Start the interactive shell by running `sald` without arguments:

```bash
./target/release/sald
>>> let x = 10
>>> x * 2
20
```

### Package Manager

Initialize a new project:

```bash
./target/release/salad new my-project
```

## Documentation

Comprehensive documentation is available in the `docs/sald-lang/index.html` file. You can open it directly in your browser to view the API reference and language guide.

[Open Documentation](docs/sald-lang/index.html)

## Example

```sald
// main.sald

class Greeter {
    fun init(name) {
        self.name = name
    }

    fun greet() {
        Console.println($"Hello, {self.name}!")
    }
}

async fun fetchInfo() {
    try {
        let res = await Http.get("https://api.github.com/zen")
        Console.println($"Github Zen: {res}")
    } catch (e) {
        Console.error("Failed to fetch:", e)
    }
}

let g = Greeter("World")
g.greet()

fetchInfo()
```

## License

This project is licensed under the MIT License.
