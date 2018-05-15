// preprocessing our search function
use errors::*;
use chrono_english::*;
use chrono::prelude::*;
use chrono::Duration;
use glob::Pattern;
use regex::{Regex,RegexBuilder,Captures};
use std::env;
use std::error::Error;

pub use GlobIgnoreCase;

static DATE_METHODS: &[&str] = &["before","after","since","on","between"];
// please place the shortest common prefix last
static PATH_METHODS: &[&str] = &["matches_ignore_case", "matches"];

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
    let seek_method_args = Regex::new(&format!("{}{}",obj,r#"\.([[:alpha:]_]+)\s*\([^\)]+\)"#))?;
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

fn preprocess_glob_patterns(text: &str) -> BoxResult<(String,Vec<(Pattern, GlobIgnoreCase)>)> {
    let mut patterns = Vec::new();
    let res = preprocess_string_arguments(text, "path", PATH_METHODS, |method, glob_str| {
        let ignore_case = if method == "matches_ignore_case" {
            GlobIgnoreCase::CaseInsensitive
        } else {
            GlobIgnoreCase::CaseSensitive
        };
        patterns.push((Pattern::new(glob_str)?, ignore_case));
        Ok((patterns.len()-1).to_string())
    })?;
    Ok((res,patterns))
}

pub fn create_filter(filter: &str, name: &str, args: &str) -> BoxResult<(String,Vec<(Pattern, GlobIgnoreCase)>)> {
    let debug = env::var("FINDR_DEBUG").is_ok();
    // be a little careful in replacing _words_
    let filter = Regex::new(r"\band\b")?.replace(filter,"&&").into_owned();
    let filter = Regex::new(r"\bor\b")?.replace(&filter,"||").into_owned();
    let filter = Regex::new(r"\bnot\b")?.replace(&filter,"!").into_owned();

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

fn word_rest(txt: &str, sep: char) -> (&str,&str) {
    if let Some(pos) = txt.find(sep) {
        let (word,rest) = (&txt[0..pos], &txt[pos+1..]);
        (word,rest.trim_left())
    } else {
        (txt,"")
    }
}

pub fn preprocess_quick_filter(glob: &str, nocase: bool) -> String {
    // The idea is to check if the filter contains spaces and then
    // to interpret the rest as either a time or a size condition.
    // May separate the glob from the condition with a semicolon
    let (mut glob,rest) = if ! glob.contains(';') {
        // However, some people use spaces in files! So the hack
        // is to check whether the glob ends with an extension or a slash
        let ext_or_slash = Regex::new(r"(\.\S+|\S/)$").unwrap().is_match(glob);
        if ! ext_or_slash {
            word_rest(&glob,' ')
        } else {
            (glob,"")
        }
    } else {
        word_rest(&glob,';')
    };
    // may need the slash to disambiguate, but it must go...
    if glob.ends_with('/') {
        glob = &glob[0..glob.len()-1]
    }
    let wildcard = if ! glob.starts_with('*') {
        // .ext beomes *.ext, file becomes */file
        if glob.starts_with('.') {"*"} else {"*/"}
    } else {
        ""
    };
    // the generated filter just calls the appropriate match method on path
    let mut filter = format!("path.{}(\"{}{}\")",
        if nocase {"matches_ignore_case"} else {"matches"},wildcard,glob
    );

    if rest.len() > 0 {
        let (op,rest) = word_rest(rest,' ');
        filter = if op == "<" || op == ">" {
            format!("{} && path.size {} {}",filter,op,rest)
        } else {
            format!("{} && date.{}(\"{}\")",filter,op,rest)
        }
    };
    if env::var("FINDR_DEBUG").is_ok() {
        println!("quick filter {}",filter);
    }
    filter
}
