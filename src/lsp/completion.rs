// Completion Provider for Sald LSP
// Provides autocomplete suggestions for keywords, built-ins, and symbols

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

/// Get keyword completions
pub fn get_keyword_completions() -> Vec<CompletionItem> {
    let keywords = [
        ("let", "Variable declaration"),
        ("const", "Constant declaration"),
        ("fun", "Function declaration"),
        ("class", "Class declaration"),
        ("if", "Conditional statement"),
        ("else", "Else branch"),
        ("while", "While loop"),
        ("for", "For-in loop"),
        ("in", "In keyword for loops"),
        ("do", "Do-while loop"),
        ("return", "Return from function"),
        ("break", "Break from loop"),
        ("continue", "Continue to next iteration"),
        ("try", "Try block"),
        ("catch", "Catch block"),
        ("throw", "Throw exception"),
        ("import", "Import module"),
        ("as", "Import alias"),
        ("namespace", "Namespace declaration"),
        ("enum", "Enum declaration"),
        ("extends", "Class inheritance"),
        ("super", "Parent class reference"),
        ("self", "Current instance reference"),
        ("async", "Async function modifier"),
        ("await", "Await async expression"),
        ("switch", "Switch expression"),
        ("default", "Default case"),
        ("true", "Boolean true"),
        ("false", "Boolean false"),
        ("null", "Null value"),
    ];

    keywords
        .iter()
        .map(|(name, doc)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some(doc.to_string()),
            ..Default::default()
        })
        .collect()
}

/// Built-in class with its methods
pub struct BuiltinClass {
    pub name: &'static str,
    pub doc: &'static str,
    pub methods: &'static [(&'static str, &'static str, &'static str)], // (name, signature, doc)
    pub properties: &'static [(&'static str, &'static str)], // (name, doc)
}

use super::symbols::{Symbol, SymbolKind};
use tower_lsp::lsp_types::Range;

/// Convert all builtin classes to Symbol format for unified handling  
pub fn get_builtin_symbols() -> Vec<Symbol> {
    BUILTIN_CLASSES.iter().map(|cls| {
        // Create method children
        let mut children: Vec<Symbol> = cls.methods.iter().map(|(name, sig, _doc)| {
            Symbol {
                name: name.to_string(),
                kind: SymbolKind::Method,
                range: Range::default(),
                selection_range: Range::default(),
                detail: Some(sig.to_string()),
                documentation: None,
                children: Vec::new(),
                type_hint: None, source_uri: None }
        }).collect();
        
        // Add properties as constants
        children.extend(cls.properties.iter().map(|(name, doc)| {
            Symbol {
                name: name.to_string(),
                kind: SymbolKind::Constant,
                range: Range::default(),
                selection_range: Range::default(),
                detail: Some(doc.to_string()),
                documentation: None,
                children: Vec::new(),
                type_hint: None, source_uri: None }
        }));
        
        Symbol {
            name: cls.name.to_string(),
            kind: SymbolKind::Class,
            range: Range::default(),
            selection_range: Range::default(),
            detail: Some(format!("{} (built-in)", cls.doc)),
            documentation: Some(cls.doc.to_string()),
            children,
            type_hint: None, source_uri: None }
    }).collect()
}

/// All built-in classes with their methods
pub static BUILTIN_CLASSES: &[BuiltinClass] = &[
    BuiltinClass {
        name: "Console",
        doc: "Console I/O operations",
        methods: &[
            ("print", "print(...args)", "Print without newline"),
            ("println", "println(...args)", "Print with newline"),
            ("input", "input(prompt?)", "Read user input"),
            ("clear", "clear()", "Clear the console"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Math",
        doc: "Mathematical functions and constants",
        methods: &[
            ("abs", "abs(n)", "Absolute value"),
            ("floor", "floor(n)", "Round down"),
            ("ceil", "ceil(n)", "Round up"),
            ("round", "round(n)", "Round to nearest"),
            ("sqrt", "sqrt(n)", "Square root"),
            ("pow", "pow(base, exp)", "Power"),
            ("sin", "sin(n)", "Sine (radians)"),
            ("cos", "cos(n)", "Cosine (radians)"),
            ("tan", "tan(n)", "Tangent (radians)"),
            ("asin", "asin(n)", "Arc sine"),
            ("acos", "acos(n)", "Arc cosine"),
            ("atan", "atan(n)", "Arc tangent"),
            ("log", "log(n)", "Natural logarithm"),
            ("log10", "log10(n)", "Base 10 logarithm"),
            ("exp", "exp(n)", "e^n"),
            ("random", "random()", "Random number 0-1"),
            ("min", "min(...args)", "Minimum value"),
            ("max", "max(...args)", "Maximum value"),
        ],
        properties: &[
            ("PI", "π = 3.14159..."),
            ("E", "e = 2.71828..."),
            ("INFINITY", "Positive infinity"),
            ("NEG_INFINITY", "Negative infinity"),
            ("NAN", "Not a number"),
        ],
    },
    BuiltinClass {
        name: "File",
        doc: "File system operations (async)",
        methods: &[
            ("read", "await read(path)", "Read file contents"),
            ("write", "await write(path, content)", "Write to file"),
            ("append", "await append(path, content)", "Append to file"),
            ("exists", "await exists(path)", "Check if path exists"),
            ("isFile", "await isFile(path)", "Check if path is file"),
            ("isDir", "await isDir(path)", "Check if path is directory"),
            ("size", "await size(path)", "Get file size in bytes"),
            ("delete", "await delete(path)", "Delete file or empty dir"),
            ("copy", "await copy(src, dst)", "Copy file"),
            ("rename", "await rename(old, new)", "Rename/move file"),
            ("mkdir", "await mkdir(path)", "Create directory"),
            ("readDir", "await readDir(path)", "List directory contents"),
            ("join", "join(...parts)", "Join path components"),
            ("dirname", "dirname(path)", "Get directory name"),
            ("basename", "basename(path)", "Get file name"),
            ("ext", "ext(path)", "Get file extension"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Timer",
        doc: "Time utilities",
        methods: &[
            ("sleep", "await sleep(ms)", "Sleep for milliseconds"),
            ("now", "now()", "Current timestamp in ms"),
            ("millis", "millis()", "Alias for now()"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Date",
        doc: "Date and time functions",
        methods: &[
            ("now", "now()", "Current datetime string"),
            ("timestamp", "timestamp()", "Unix timestamp in seconds"),
            ("year", "year()", "Current year"),
            ("month", "month()", "Current month (1-12)"),
            ("day", "day()", "Current day (1-31)"),
            ("hour", "hour()", "Current hour (0-23)"),
            ("minute", "minute()", "Current minute (0-59)"),
            ("second", "second()", "Current second (0-59)"),
            ("format", "format(pattern)", "Format datetime"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Json",
        doc: "JSON parsing and serialization",
        methods: &[
            ("parse", "parse(json)", "Parse JSON string to value"),
            ("stringify", "stringify(value, indent?)", "Convert value to JSON"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Path",
        doc: "Path utilities",
        methods: &[
            ("join", "join(...parts)", "Join path components"),
            ("dirname", "dirname(path)", "Get directory name"),
            ("basename", "basename(path)", "Get file name"),
            ("extname", "extname(path)", "Get extension with dot"),
            ("isAbsolute", "isAbsolute(path)", "Check if absolute path"),
            ("exists", "exists(path)", "Check if path exists"),
            ("normalize", "normalize(path)", "Normalize path"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Process",
        doc: "Process and environment operations",
        methods: &[
            ("args", "args()", "Command line arguments"),
            ("env", "env(name)", "Get environment variable"),
            ("cwd", "cwd()", "Current working directory"),
            ("chdir", "chdir(path)", "Change working directory"),
            ("exit", "exit(code?)", "Exit process"),
            ("exec", "exec(command)", "Execute shell command"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Http",
        doc: "HTTP client and server",
        methods: &[
            ("get", "await get(url)", "HTTP GET request"),
            ("post", "await post(url, body?)", "HTTP POST request"),
            ("put", "await put(url, body?)", "HTTP PUT request"),
            ("delete", "await delete(url)", "HTTP DELETE request"),
            ("Server", "Server()", "Create HTTP server"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Type",
        doc: "Type checking utilities",
        methods: &[
            ("of", "of(value)", "Get type name as string"),
            ("isNumber", "isNumber(value)", "Check if number"),
            ("isString", "isString(value)", "Check if string"),
            ("isBoolean", "isBoolean(value)", "Check if boolean"),
            ("isNull", "isNull(value)", "Check if null"),
            ("isArray", "isArray(value)", "Check if array"),
            ("isFunction", "isFunction(value)", "Check if function"),
            ("isClass", "isClass(value)", "Check if class"),
            ("isInstance", "isInstance(value)", "Check if instance"),
            ("isDict", "isDict(value)", "Check if dictionary"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "System",
        doc: "System information",
        methods: &[
            ("os", "os()", "Operating system name"),
            ("arch", "arch()", "CPU architecture"),
            ("family", "family()", "OS family (unix/windows)"),
            ("cpuCount", "cpuCount()", "Number of CPU cores"),
            ("hostname", "hostname()", "Computer hostname"),
            ("osVersion", "osVersion()", "OS version string"),
            ("kernelVersion", "kernelVersion()", "Kernel version"),
            ("totalMemory", "totalMemory()", "Total RAM in bytes"),
            ("usedMemory", "usedMemory()", "Used RAM in bytes"),
            ("freeMemory", "freeMemory()", "Free RAM in bytes"),
            ("cpuName", "cpuName()", "CPU model name"),
            ("cpuUsage", "cpuUsage()", "CPU usage percentage"),
            ("uptime", "uptime()", "System uptime in seconds"),
            ("bootTime", "bootTime()", "Boot time as Unix timestamp"),
            ("info", "info()", "All system info as dictionary"),
            ("getenv", "getenv(name)", "Get environment variable"),
            ("setenv", "setenv(name, value)", "Set environment variable"),
            ("envs", "envs()", "All environment variables"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Array",
        doc: "Array operations",
        methods: &[
            ("range", "range(end) / range(start, end, step?)", "Create range array"),
            ("length", "length()", "Get array length"),
            ("push", "push(item)", "Add item to end"),
            ("pop", "pop()", "Remove and return last item"),
            ("shift", "shift()", "Remove and return first item"),
            ("unshift", "unshift(item)", "Add item to beginning"),
            ("first", "first()", "Get first item"),
            ("last", "last()", "Get last item"),
            ("get", "get(index)", "Get item at index"),
            ("set", "set(index, value)", "Set item at index"),
            ("at", "at(index)", "Get item at index (negative allowed)"),
            ("contains", "contains(item)", "Check if contains item"),
            ("indexOf", "indexOf(item)", "Find index of item"),
            ("lastIndexOf", "lastIndexOf(item)", "Find last index of item"),
            ("join", "join(separator)", "Join items to string"),
            ("reverse", "reverse()", "Reverse in-place"),
            ("toReversed", "toReversed()", "Return reversed copy"),
            ("slice", "slice(start, end?)", "Get sub-array"),
            ("splice", "splice(start, deleteCount, ...items)", "Remove/insert items"),
            ("concat", "concat(other)", "Concatenate arrays"),
            ("clear", "clear()", "Remove all items"),
            ("isEmpty", "isEmpty()", "Check if empty"),
            ("fill", "fill(value, start?, end?)", "Fill with value"),
            ("flat", "flat(depth?)", "Flatten nested arrays"),
            ("flatMap", "flatMap(fn)", "Map then flatten"),
            ("map", "map(fn)", "Transform each item"),
            ("filter", "filter(fn)", "Filter items"),
            ("forEach", "forEach(fn)", "Iterate items"),
            ("reduce", "reduce(fn, initial?)", "Reduce to single value"),
            ("find", "find(fn)", "Find first matching item"),
            ("findIndex", "findIndex(fn)", "Find index of first match"),
            ("findLast", "findLast(fn)", "Find last matching item"),
            ("findLastIndex", "findLastIndex(fn)", "Find index of last match"),
            ("some", "some(fn)", "Check if any match"),
            ("every", "every(fn)", "Check if all match"),
            ("sort", "sort(fn?)", "Sort in-place"),
            ("toSorted", "toSorted(fn?)", "Return sorted copy"),
            ("includes", "includes(item)", "Check if includes item"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Dict",
        doc: "Dictionary operations",
        methods: &[
            ("length", "length()", "Get number of keys"),
            ("keys", "keys()", "Get all keys as array"),
            ("values", "values()", "Get all values as array"),
            ("entries", "entries()", "Get [key, value] pairs"),
            ("get", "get(key, default?)", "Get value for key"),
            ("set", "set(key, value)", "Set key-value pair"),
            ("has", "has(key)", "Check if key exists"),
            ("remove", "remove(key)", "Remove key"),
            ("clear", "clear()", "Remove all keys"),
            ("isEmpty", "isEmpty()", "Check if empty"),
            ("toString", "toString()", "Convert to string"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "String",
        doc: "String operations",
        methods: &[
            ("length", "length()", "Get string length"),
            ("upper", "upper()", "Convert to uppercase"),
            ("lower", "lower()", "Convert to lowercase"),
            ("trim", "trim()", "Remove whitespace from both ends"),
            ("trimStart", "trimStart()", "Remove leading whitespace"),
            ("trimEnd", "trimEnd()", "Remove trailing whitespace"),
            ("contains", "contains(substr)", "Check if contains substring"),
            ("includes", "includes(substr)", "Check if contains substring"),
            ("startsWith", "startsWith(prefix)", "Check prefix"),
            ("endsWith", "endsWith(suffix)", "Check suffix"),
            ("charAt", "charAt(index)", "Get character at index"),
            ("charCodeAt", "charCodeAt(index)", "Get char code at index"),
            ("substring", "substring(start, end?)", "Get substring"),
            ("slice", "slice(start, end?)", "Get slice (negative allowed)"),
            ("indexOf", "indexOf(substr)", "Find first index of substring"),
            ("lastIndexOf", "lastIndexOf(substr)", "Find last index of substring"),
            ("replace", "replace(old, new)", "Replace first occurrence"),
            ("replaceAll", "replaceAll(old, new)", "Replace all occurrences"),
            ("split", "split(separator)", "Split to array"),
            ("repeat", "repeat(count)", "Repeat string n times"),
            ("padStart", "padStart(length, pad?)", "Pad start to length"),
            ("padEnd", "padEnd(length, pad?)", "Pad end to length"),
            ("concat", "concat(...strings)", "Concatenate strings"),
            ("isDigit", "isDigit()", "Check if single digit"),
            ("isAlpha", "isAlpha()", "Check if alphabetic"),
            ("isAlphanumeric", "isAlphanumeric()", "Check if alphanumeric"),
            ("isWhitespace", "isWhitespace()", "Check if whitespace"),
            ("toNumber", "toNumber()", "Parse as number"),
            ("toString", "toString()", "Convert to string"),
        ],
        properties: &[],
    },
    BuiltinClass {
        name: "Ffi",
        doc: "Foreign Function Interface",
        methods: &[
            ("load", "load(path)", "Load dynamic library"),
            ("callback", "callback(fn)", "Register callback function"),
            ("removeCallback", "removeCallback(id)", "Unregister callback"),
        ],
        properties: &[
            ("NULL", "Null pointer (0)"),
            ("INVOKER", "Callback invoker pointer"),
        ],
    },
];

