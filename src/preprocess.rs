// preprocessing our search function
use errors::*;
use chrono_english::*;
use chrono::prelude::*;
use chrono::Duration;
use glob::Pattern;
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

fn first_char(s: &str) -> char {
    s.chars().next().unwrap()
}

// massage any string arguments of known `methods` of the object `obj`
fn preprocess_string_arguments<C>(text: &str, obj: &str, methods: &[&str], mut process: C) -> BoxResult<String>
where C: FnMut(&str,&str) -> BoxResult<String>  {
    let mut s = text;
    let mut res = String::new();
    let obj_dot = format!("{}.",obj);
    while let Some(start_obj) = s.find(&obj_dot) {
        let start_obj = start_obj + obj_dot.len();
        res += &s[0..start_obj]; // everything up to OBJ.
        s = &s[start_obj..];
        let midx = s.find(|c:char| ! (c.is_alphanumeric() || c == '_')).unwrap();
        if (&s[midx..]).starts_with('(') {
            let method = &s[0..midx];
            if ! methods.contains(&method) {
                return err_io(&format!("unknown {} method {}",obj,method));
            }
            res += &s[0..midx];
            res.push('(');
            s = &s[midx+1..];
            loop {
                let ch = first_char(s);
                if ch == '"' {
                    s = &s[1..];
                    if let Some(ends) = s.find('"') {
                        // the actual substitution
                        let subst = process(method,&s[0..ends])?;
                        res += &subst;
                        s = &s[ends+1..]; // just after "
                    } else {
                        return err_io("unterminated string");
                    }
                } else {
                    return err_io("bad argument - must be string");
                }
                // either , or )
                let ch = first_char(s);
                res.push(ch);
                s = &s[1..];
                if ch == ')' {
                    break;
                }
            }
        } else {
           // just pass through fields (for now)
           res += &s[0..midx+1];
           s = &s[midx+1..];
        }
    }
    res += s;
    Ok(res)
}

// convert date strings into Unix timestamps using chrono-english
fn preprocess_dates(text: &str) -> BoxResult<String> {
    let dialect = if env::var("FINDR_US").is_ok() {
        Dialect::Us
    } else {
        Dialect::Uk
    };
    preprocess_string_arguments(text,"date",DATE_METHODS,|method,datestr| {
        let dt = parse_date_string(datestr,Local::now(),dialect)?;
        Ok(if method == "on" {
            // "on" is special - the datestr expands to _two_ timestamps spanning the day
            let day_start = dt.with_hour(0).unwrap().with_minute(0).unwrap();
            let day_end = day_start + Duration::days(1);
            format!("{},{}",day_start.timestamp(),day_end.timestamp())
        } else {
            dt.timestamp().to_string()
        })
    })
}

fn preprocess_glob_patterns(text: &str) -> BoxResult<(String,Vec<Pattern>)> {
    let mut patterns = Vec::new();
    let res = preprocess_string_arguments(text,"path",&["matches"],|_,glob_str| {
        patterns.push(Pattern::new(glob_str)?);
        Ok((patterns.len()-1).to_string())
    })?;
    Ok((res,patterns))
}

pub fn create_filter(filter: &str, name: &str, args: &str) -> BoxResult<(String,Vec<Pattern>)> {
    let debug = env::var("FINDR_DEBUG").is_ok();
    let filter = filter.to_lowercase() + " ";
    let filter = filter.replace(" and "," && ").replace(" or "," || ").replace(" not "," ! ");
    let res = preprocess_numbers(&filter)?;
    if debug { println!("numbers {}",res); }
    let res = preprocess_dates(&res)?;
    if debug { println!("dates {}",res); }
    let (res,patterns) = preprocess_glob_patterns(&res)?;
    let fun = format!("fn {}({}) {{\n\t{}\n}}\n",name,args,res);
    if debug {
        println!("fun {}",fun);
        if patterns.len() > 0 {
            println!("globs {:?}",patterns);
        }
    }
    Ok((fun,patterns))
}
