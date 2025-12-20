// Crypto built-in module
// Provides cryptographic operations: hashing, HMAC, UUID, random, base64

use super::{check_arity, get_number_arg, get_string_arg};
use crate::vm::value::{Class, NativeStaticFn, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::{Engine, engine::general_purpose};
use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha512, Digest};

type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

pub fn create_crypto_class() -> Class {
    let mut static_methods: HashMap<String, NativeStaticFn> = HashMap::new();

    static_methods.insert("hash".to_string(), crypto_hash);
    static_methods.insert("hmac".to_string(), crypto_hmac);
    static_methods.insert("uuid".to_string(), crypto_uuid);
    static_methods.insert("randomBytes".to_string(), crypto_random_bytes);
    static_methods.insert("randomInt".to_string(), crypto_random_int);
    static_methods.insert("base64Encode".to_string(), crypto_base64_encode);
    static_methods.insert("base64Decode".to_string(), crypto_base64_decode);

    Class::new_with_static("Crypto", static_methods)
}

/// Crypto.hash(algorithm, data) - Hash data with specified algorithm
/// Supported algorithms: "sha256", "sha512", "md5", "sha1"
fn crypto_hash(args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    let algorithm = get_string_arg(&args[0], "algorithm")?.to_lowercase();
    let data = get_string_arg(&args[1], "data")?;

    let hash_hex = match algorithm.as_str() {
        "sha256" => {
            let mut hasher = Sha256::new();
            hasher.update(data.as_bytes());
            hex::encode(hasher.finalize())
        }
        "sha512" => {
            let mut hasher = Sha512::new();
            hasher.update(data.as_bytes());
            hex::encode(hasher.finalize())
        }
        "md5" => {
            let digest = md5::compute(data.as_bytes());
            hex::encode(digest.as_ref())
        }
        "sha1" => {
            use sha1::{Sha1, Digest as Sha1Digest};
            let mut hasher = Sha1::new();
            hasher.update(data.as_bytes());
            hex::encode(hasher.finalize())
        }
        _ => return Err(format!("Unsupported hash algorithm: {}. Use sha256, sha512, md5, or sha1", algorithm)),
    };

    Ok(Value::String(Arc::new(hash_hex)))
}

/// Crypto.hmac(algorithm, key, data) - HMAC signing
fn crypto_hmac(args: &[Value]) -> Result<Value, String> {
    check_arity(3, args.len())?;
    let algorithm = get_string_arg(&args[0], "algorithm")?.to_lowercase();
    let key = get_string_arg(&args[1], "key")?;
    let data = get_string_arg(&args[2], "data")?;

    let hmac_hex = match algorithm.as_str() {
        "sha256" => {
            let mut mac = HmacSha256::new_from_slice(key.as_bytes())
                .map_err(|e| format!("HMAC error: {}", e))?;
            mac.update(data.as_bytes());
            hex::encode(mac.finalize().into_bytes())
        }
        "sha512" => {
            let mut mac = HmacSha512::new_from_slice(key.as_bytes())
                .map_err(|e| format!("HMAC error: {}", e))?;
            mac.update(data.as_bytes());
            hex::encode(mac.finalize().into_bytes())
        }
        _ => return Err(format!("Unsupported HMAC algorithm: {}. Use sha256 or sha512", algorithm)),
    };

    Ok(Value::String(Arc::new(hmac_hex)))
}

/// Crypto.uuid() - Generate UUID v4
fn crypto_uuid(_args: &[Value]) -> Result<Value, String> {
    let id = uuid::Uuid::new_v4().to_string();
    Ok(Value::String(Arc::new(id)))
}

/// Crypto.randomBytes(length) - Generate array of random bytes
fn crypto_random_bytes(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let length = get_number_arg(&args[0], "length")? as usize;
    
    if length > 1024 * 1024 {
        return Err("randomBytes length cannot exceed 1MB".to_string());
    }

    use rand::Rng;
    let mut rng = rand::rng();
    let bytes: Vec<Value> = (0..length)
        .map(|_| Value::Number(rng.random::<u8>() as f64))
        .collect();

    Ok(Value::Array(Arc::new(Mutex::new(bytes))))
}

/// Crypto.randomInt(min, max) - Generate random integer in range [min, max]
fn crypto_random_int(args: &[Value]) -> Result<Value, String> {
    check_arity(2, args.len())?;
    let min = get_number_arg(&args[0], "min")? as i64;
    let max = get_number_arg(&args[1], "max")? as i64;

    if min > max {
        return Err("min cannot be greater than max".to_string());
    }

    use rand::Rng;
    let mut rng = rand::rng();
    let value = rng.random_range(min..=max);

    Ok(Value::Number(value as f64))
}

/// Crypto.base64Encode(data) - Encode string to base64
fn crypto_base64_encode(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let data = get_string_arg(&args[0], "data")?;
    let encoded = general_purpose::STANDARD.encode(data.as_bytes());
    Ok(Value::String(Arc::new(encoded)))
}

/// Crypto.base64Decode(data) - Decode base64 to string
fn crypto_base64_decode(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let data = get_string_arg(&args[0], "data")?;
    
    let bytes = general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("Base64 decode error: {}", e))?;
    
    let decoded = String::from_utf8(bytes)
        .map_err(|e| format!("UTF-8 decode error: {}", e))?;
    
    Ok(Value::String(Arc::new(decoded)))
}
