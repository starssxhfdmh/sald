# Sald Programming Language

Syntax highlighting and language support for the **Sald** programming language in Visual Studio Code.

## Features

- ✅ Full syntax highlighting
- ✅ Keyword recognition (`if`, `else`, `while`, `for`, `fun`, `class`, etc.)
- ✅ String highlighting (single, double, multiline, raw, format strings)
- ✅ Number highlighting (integers, floats, hex, binary)
- ✅ Comment highlighting (single-line `//` and block `/* */`)
- ✅ Built-in class highlighting (`Console`, `Math`, `Http`, `File`, etc.)
- ✅ Operator highlighting
- ✅ Lambda expression support (`|x| x * 2`)
- ✅ Auto-closing brackets and quotes
- ✅ Comment toggling

## Installation

### From VSIX

1. Download the `.vsix` file
2. In VS Code, press `Ctrl+Shift+P`
3. Type "Install from VSIX" and select the file

### Manual Installation

Copy the `sald-programming-language` folder to:
- **Windows**: `%USERPROFILE%\.vscode\extensions\`
- **macOS/Linux**: `~/.vscode/extensions/`

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

## License

MIT
