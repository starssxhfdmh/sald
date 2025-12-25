



use super::{check_arity, get_string_arg};
use crate::vm::value::{Class, NativeStaticFn, Value};
use rustc_hash::FxHashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn create_date_class() -> Class {
    let mut static_methods: FxHashMap<String, NativeStaticFn> = FxHashMap::default();

    static_methods.insert("now".to_string(), date_now);
    static_methods.insert("timestamp".to_string(), date_timestamp);
    static_methods.insert("year".to_string(), date_year);
    static_methods.insert("month".to_string(), date_month);
    static_methods.insert("day".to_string(), date_day);
    static_methods.insert("hour".to_string(), date_hour);
    static_methods.insert("minute".to_string(), date_minute);
    static_methods.insert("second".to_string(), date_second);
    static_methods.insert("format".to_string(), date_format);

    Class::new_with_static("Date", static_methods)
}

fn get_current_datetime() -> (i32, u32, u32, u32, u32, u32, u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let secs = now.as_secs();
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;

    let hour = (time_of_day / 3600) as u32;
    let minute = ((time_of_day % 3600) / 60) as u32;
    let second = (time_of_day % 60) as u32;

    let mut days = days_since_epoch as i64;
    let mut year = 1970i32;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let days_in_months: [i64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for days_in_month in days_in_months.iter() {
        if days < *days_in_month {
            break;
        }
        days -= days_in_month;
        month += 1;
    }

    let day = (days + 1) as u32;
    (year, month, day, hour, minute, second, secs)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn date_now(args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    let (year, month, day, hour, minute, second, _) = get_current_datetime();
    Ok(Value::String(Rc::from(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hour, minute, second
    ))))
}

fn date_timestamp(args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    let (_, _, _, _, _, _, secs) = get_current_datetime();
    Ok(Value::Number(secs as f64))
}

fn date_year(args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    let (year, _, _, _, _, _, _) = get_current_datetime();
    Ok(Value::Number(year as f64))
}

fn date_month(args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    let (_, month, _, _, _, _, _) = get_current_datetime();
    Ok(Value::Number(month as f64))
}

fn date_day(args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    let (_, _, day, _, _, _, _) = get_current_datetime();
    Ok(Value::Number(day as f64))
}

fn date_hour(args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    let (_, _, _, hour, _, _, _) = get_current_datetime();
    Ok(Value::Number(hour as f64))
}

fn date_minute(args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    let (_, _, _, _, minute, _, _) = get_current_datetime();
    Ok(Value::Number(minute as f64))
}

fn date_second(args: &[Value]) -> Result<Value, String> {
    check_arity(0, args.len())?;
    let (_, _, _, _, _, second, _) = get_current_datetime();
    Ok(Value::Number(second as f64))
}

fn date_format(args: &[Value]) -> Result<Value, String> {
    check_arity(1, args.len())?;
    let format_str = get_string_arg(&args[0], "format")?;
    let (year, month, day, hour, minute, second, _) = get_current_datetime();

    let result = format_str
        .replace("YYYY", &format!("{:04}", year))
        .replace("MM", &format!("{:02}", month))
        .replace("DD", &format!("{:02}", day))
        .replace("HH", &format!("{:02}", hour))
        .replace("mm", &format!("{:02}", minute))
        .replace("ss", &format!("{:02}", second));

    Ok(Value::String(Rc::from(result)))
}
