// preprocessing our search function
use errors::*;
use chrono_english::*;
use chrono::prelude::*;
use chrono::Duration;
use std::env;

static DATE_METHODS: &[&str] = &["before","after","since","on","between"];

const POSTFIXES: &[char] = &['k','m','g'];

fn digit(ch: char) -> bool {
    ch.is_digit(10)
}

fn non_number(ch: char) -> bool {
    ! ch.is_digit(10) && ch != '.'
}

// replace number literals with postfixes like '256kb' and '0.5mb'
// with corresponding integers.
fn preprocess_numbers(text: &str) -> BoxResult<String> {
    let mut s = text;
    let mut res = String::new();
    while let Some(start_num) = s.find(digit) {
        res += &s[0..start_num];
        s = &s[start_num..]; // "245kb..."
        if let Some(mut end_num) = s.find(non_number) {
            let nums = &s[0..end_num];
            let mut iter = (&s[end_num..]).chars();
            let initial = iter.next().unwrap(); // cool because always extra space...
            let num: f64 = nums.parse()?;
            s = &s[end_num..];
            if POSTFIXES.contains(&initial) {
                let mult: u64 = match initial {
                    'k' => 1024,
                    'm' => 1024*1024,
                    'g' => 1024*1024*1024,
                    _ => unreachable!(),
                };
                let skip = if iter.next().unwrap() == 'b' { 2 } else { 1 };
                let num = num * mult as f64;
                res += &((num as u64).to_string());
                s = &s[skip..];
            } else {
                res += nums;
            }
        }
    }
    res += s;
    Ok(res)
}

fn preprocess_dates(text: &str) -> BoxResult<String> {
    let dialect = if env::var("FINDR_US").is_ok() {
        Dialect::Us
    } else {
        Dialect::Uk
    };
    let mut s = text;
    let mut res = String::new();
    let date = "date.";
    while let Some(start_date) = s.find(date) {
        let start_date = start_date + date.len();
        res += &s[0..start_date]; // everything up to "date."
        s = &s[start_date..];
        if let Some(midx) = s.find('(') {
            let method = &s[0..midx];
            if ! DATE_METHODS.contains(&method) {
                return err_io(&format!("unknown date method {}",method));
            }
            res += &s[0..midx];
            res.push('(');
            s = &s[midx+1..];
            loop {
                let ch = s.chars().next().unwrap();
                if ch == '"' {
                    s = &s[1..];
                    if let Some(ends) = s.find('"') {
                        let datestr = &s[0..ends];
                        // the actual substitution
                        let dt = parse_date_string(datestr,Local::now(),dialect)?;
                        if method == "on" {
                            // "on" is special - the datestr expands to _two_ timestamps spanning the day
                            let day_start = dt.with_hour(0).unwrap().with_minute(0).unwrap();
                            let day_end = day_start + Duration::days(1);
                            res += &day_start.timestamp().to_string();
                            res += ",";
                            res += &day_end.timestamp().to_string();
                        } else {
                            res += &dt.timestamp().to_string();
                        }
                        s = &s[ends+1..]; // just after "
                    } else {
                        return err_io("unterminated string");
                    }
                } else {
                    return err_io("bad date argument - must be string");
                }
                // either , or )
                let ch = s.chars().next().unwrap();
                res.push(ch);
                s = &s[1..];
                if ch == ')' {
                    break;
                }
            }
        } else {
            return err_io("bad date format");
        }
    }
    res += s;
    Ok(res)
}

pub fn create_filter(filter: &str) -> BoxResult<String> {
    let debug = env::var("FINDR_DEBUG").is_ok();
    let filter = filter.to_lowercase() + " ";
    let filter = filter.replace(" and "," && ").replace(" or "," || ").replace(" not "," ! ");
    let res = preprocess_numbers(&filter)?;
    if debug { println!("numbers {}",res); }
    let res_d = preprocess_dates(&res)?;
    if debug { println!("dates {}",res_d); }
    let mut fun = String::new();
    fun += "fn filter(path,date,mode) {\n\t";
    fun += &res_d;
    fun += "\n}\n";
    Ok(fun)
}


