<p align="center">
  <img src="docs/logo.png" alt="Sald Logo" width="200">
</p>

<h1 align="center">Sald</h1>

<p align="center">
  A modern dynamic programming language built with Rust.
</p>

---

## Features

- Class-based OOP with inheritance
- Async/await for asynchronous programming
- Standard library: File, Http, Json, System, Process, etc.  
- Package manager (`salad`)
- Language server (`sald-lsp`)
- REPL for quick experimentation

## Installation

### Quick Install

**Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/starssxhfdmh/sald/master/install.sh | bash
```

**Windows:**
```powershell
irm https://raw.githubusercontent.com/starssxhfdmh/sald/master/install.ps1 | iex
```

### Build from Source

```bash
git clone https://github.com/starssxhfdmh/sald.git
cd sald && cargo build --release
```

## Quick Start

```bash
# Run a script
sald script.sald

# Start REPL
sald

# Create new project
salad new my-project
```

## Example

```sald
class Greeter {
    fun init(self, name) {
        self.name = name
    }

    fun greet(self) {
        Console.println($"Hello, {self.name}!")
    }
}

let g = Greeter("World")
g.greet()
```

## Documentation

See [DOCS.md](DOCS.md) for the complete language reference.

## License

MIT License
