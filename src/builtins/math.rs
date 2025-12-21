// Math built-in class
// Provides: PI, E, abs, floor, ceil, round, sqrt, pow, sin, cos, tan, random, min, max

use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;

pub fn create_math_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();
    let mut static_fields: FxHashMap<String, Value> = FxHashMap::default();

    // Static methods
    static_methods.insert("abs".to_string(), math_abs);
    static_methods.insert("floor".to_string(), math_floor);
    static_methods.insert("ceil".to_string(), math_ceil);
    static_methods.insert("round".to_string(), math_round);
    static_methods.insert("sqrt".to_string(), math_sqrt);
    static_methods.insert("pow".to_string(), math_pow);
    static_methods.insert("sin".to_string(), math_sin);
    static_methods.insert("cos".to_string(), math_cos);
    static_methods.insert("tan".to_string(), math_tan);
    static_methods.insert("asin".to_string(), math_asin);
    static_methods.insert("acos".to_string(), math_acos);
    static_methods.insert("atan".to_string(), math_atan);
    static_methods.insert("log".to_string(), math_log);
    static_methods.insert("log10".to_string(), math_log10);
    static_methods.insert("exp".to_string(), math_exp);
    static_methods.insert("random".to_string(), math_random);
    static_methods.insert("min".to_string(), math_min);
    static_methods.insert("max".to_string(), math_max);

    // Static fields (constants)
    static_fields.insert("PI".to_string(), Value::Number(std::f64::consts::PI));
    static_fields.insert("E".to_string(), Value::Number(std::f64::consts::E));
    static_fields.insert("INFINITY".to_string(), Value::Number(f64::INFINITY));
    static_fields.insert("NEG_INFINITY".to_string(), Value::Number(f64::NEG_INFINITY));
    static_fields.insert("NAN".to_string(), Value::Number(f64::NAN));

    Class::new_with_static_and_fields("Math", static_methods, static_fields)
}

fn get_number(args: &[Value], idx: usize, name: &str) -> Result<f64, String> {
    if idx >= args.len() {
        return Err(format!("Expected at least {} argument(s)", idx + 1));
    }
    match &args[idx] {
        Value::Number(n) => Ok(*n),
        _ => Err(format!(
            "Argument '{}' must be a number, got {}",
            name,
            args[idx].type_name()
        )),
    }
}

fn math_abs(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.abs()))
}

fn math_floor(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.floor()))
}

fn math_ceil(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.ceil()))
}

fn math_round(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.round()))
}

fn math_sqrt(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.sqrt()))
}

fn math_pow(args: &[Value]) -> Result<Value, String> {
    let base = get_number(args, 0, "base")?;
    let exp = get_number(args, 1, "exponent")?;
    Ok(Value::Number(base.powf(exp)))
}

fn math_sin(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.sin()))
}

fn math_cos(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.cos()))
}

fn math_tan(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.tan()))
}

fn math_asin(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.asin()))
}

fn math_acos(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.acos()))
}

fn math_atan(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.atan()))
}

fn math_log(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.ln()))
}

fn math_log10(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.log10()))
}

fn math_exp(args: &[Value]) -> Result<Value, String> {
    let n = get_number(args, 0, "n")?;
    Ok(Value::Number(n.exp()))
}

fn math_random(_args: &[Value]) -> Result<Value, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use rand::Rng;
        let mut rng = rand::rng();
        let random: f64 = rng.random();
        Ok(Value::Number(random))
    }
    
    #[cfg(target_arch = "wasm32")]
    {
        // Use getrandom for WASM (with js feature)
        use getrandom::getrandom;
        let mut buf = [0u8; 8];
        getrandom(&mut buf).map_err(|e| e.to_string())?;
        let random = u64::from_le_bytes(buf) as f64 / u64::MAX as f64;
        Ok(Value::Number(random))
    }
}

fn math_min(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected at least 1 argument but got 0".to_string());
    }
    let mut min_val = get_number(args, 0, "n")?;
    for (i, arg) in args.iter().enumerate().skip(1) {
        let n = match arg {
            Value::Number(n) => *n,
            _ => {
                return Err(format!(
                    "Argument {} must be a number, got {}",
                    i,
                    arg.type_name()
                ))
            }
        };
        if n < min_val {
            min_val = n;
        }
    }
    Ok(Value::Number(min_val))
}

fn math_max(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("Expected at least 1 argument but got 0".to_string());
    }
    let mut max_val = get_number(args, 0, "n")?;
    for (i, arg) in args.iter().enumerate().skip(1) {
        let n = match arg {
            Value::Number(n) => *n,
            _ => {
                return Err(format!(
                    "Argument {} must be a number, got {}",
                    i,
                    arg.type_name()
                ))
            }
        };
        if n > max_val {
            max_val = n;
        }
    }
    Ok(Value::Number(max_val))
}
