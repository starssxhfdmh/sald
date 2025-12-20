# Sald Programming Language

Syntax highlighting and full language support for the **Sald** programming language in Visual Studio Code.


## Installation

### From VSIX

1. Download the `.vsix` file
2. In VS Code, press `Ctrl+Shift+P`
3. Type "Install from VSIX" and select the file

### From Marketplace

Search for "Sald Programming Language" in the VS Code Extensions panel.

## LSP Setup

The extension will automatically:

1. Look for `sald-lsp` in `~/.sald/bin/` (installed by the Sald installer)
2. If not found, prompt you to **download it automatically**
3. Or you can configure a custom path in settings

### Manual Configuration

If you need to specify a custom LSP path:

1. Open VS Code Settings (`Ctrl+,`)
2. Search for "sald.lsp.path"
3. Enter the path to your `sald-lsp` executable

## Commands

Access via Command Palette (`Ctrl+Shift+P`):

| Command | Description |
|---------|-------------|
| `Sald: Run Current File` | Run the active `.sald` file (also F5) |
| `Sald: Start Language Server` | Start the LSP server |
| `Sald: Stop Language Server` | Stop the LSP server |
| `Sald: Restart Language Server` | Restart the LSP server |
| `Sald: Update Language Server` | Download latest LSP version |
| `Sald: Show Output Logs` | Show extension logs |
| `Sald: Configure LSP Executable Path` | Open settings to configure LSP path |
| `Sald: Analyze All Workspace Files` | Run diagnostics on all `.sald` files |

## Snippets

Type these prefixes and press Tab:

| Prefix | Description |
|--------|-------------|
| `fun` | Function declaration |
| `afun` | Async function |
| `class` | Class declaration |
| `for` | For-in loop |
| `forr` | For range loop |
| `if` / `ife` | If / If-else |
| `try` | Try-catch block |
| `lam` | Lambda expression |
| `log` | Console.println |
| `httpserver` | HTTP server boilerplate |
| `imp` | Import statement |

## Example

```sald
// Hello World in Sald
Console.println("Hello, World!")

// Classes and functions
class Person {
    fun init(self, name) {
        self.name = name
    }
    
    fun greet(self) {
        Console.println($"Hello, I'm {self.name}!")
    }
}

let person = Person("Alice")
person.greet()

// Async/await
async fun fetchData() {
    let data = await Http.get("https://api.example.com")
    return Json.parse(data)
}

// Lambda functions
let numbers = [1, 2, 3, 4, 5]
let doubled = numbers.map(|x| x * 2)
```

## Requirements

- VS Code 1.75.0 or later
- Sald runtime for running files (optional)

## License

MIT
