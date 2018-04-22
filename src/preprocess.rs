// preprocessing our search function
use errors::*;
use chrono_english::*;
use chrono::prelude::*;
use chrono::Duration;
use glob::Pattern;
use regex::{Regex,RegexBuilder,Captures};
use std::env;
use std::error::Error;

static DATE_METHODS: &[&str] = &["before","after","since","on","between"];

// replace number literals with postfixes like '256kb' and '0.5mb'
// with corresponding integers.
fn preprocess_numbers(text: &str) -> BoxResult<String> {
    let number_with_postfix = RegexBuilder::new(r#"(\d+(\.\d+)*)(k|m|g)b*"#).case_insensitive(true).build()?;
    let mut conversion_error: Option<String> = None;
    let res = number_with_postfix.replace_all(text,|caps: &Captures| {
        let nums = &caps[1];
        let num = match nums.parse::<f64>() {
            Ok(x) => x,
            Err(e) => {
                conversion_error = Some(e.description().into());
                return "".into();
            }
        };
        let postfix = &caps[3];
        let mult: u64 = match postfix {
            "k" | "K" => 1024,
            "m" | "M" => 1024*1024,
            "g" | "G" => 1024*1024*1024,
            _ => unreachable!(),
        };
        let num = num * mult as f64;
        num.to_string()
    });
    if let Some(err) = conversion_error {
        err_io(&err)
    } else {
        Ok(res.into_owned())
    }
}

// massage any string arguments of known `methods` of the object `obj`
fn preprocess_string_arguments<C>(text: &str, obj: &str, methods: &[&str], mut process: C) -> BoxResult<String>
where C: FnMut(&str,&str) -> BoxResult<String>  {
    let seek_method_args = Regex::new(&format!("{}{}",obj,r#"\.([[:alpha:]]+)\s*\([^\)]+\)"#))?;
    let seek_string = Regex::new(r#"\s*"([^"]+)"\s*"#)?;
    let mut possible_error: Option<String> = None;
    let res = seek_method_args.replace_all(text, |caps: &Captures| {
        let method = &caps[1];
        if ! methods.contains(&method) {
            possible_error = Some(format!("unknown {} method {}: available {:?}",obj,method,methods));
            return "".into();
        }
        seek_string.replace_all(&caps[0], |caps: &Captures| {
            match process(method,&caps[1]) {
                Ok(s) => s,
                Err(e) => {
                    possible_error = Some(e.description().into());
                    return "".into();
                }
            }
        }).into_owned()
    });
    if let Some(err) = possible_error {
        err_io(&err)
    } else {
        Ok(res.into_owned())
    }
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
    let filter = filter.to_string() + " ";
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
