// preprocessing any command
use errors::*;
use std::env;
use std::process::Command;

fn next_percent(s: &str) -> Option<(&str,&str,&str)> {
    if let Some(startp) = s.find("%(") {
        let before = &s[0..startp];
        let after = &s[startp+2..]; // skip %(
        let endp = after.find(')').expect("expected closing ) after %(");
        let subst = &after[0..endp];
        let rest = &after[endp+1..];
        Some((before,subst,rest))
    } else {
        None
    }
}

fn percent_subst(s: &str) -> String {
    let mut s = s;
    let mut buf = "return ".to_string();
    while let Some((before,subst,rest)) = next_percent(s) {
        //println!("'{}' '{}' '{}'", before,subst,rest);
        buf += &format!("{:?} + ", before);
        buf += subst;
        if rest.len() > 0 {
            buf += " + ";
        }
        s = rest;
    }
    if s.len() > 0 {
        buf += &format!("{:?}", s);
    }
    buf.push(';');
    buf
}

pub fn command(cmd: &str) -> Option<String> {
    if !cmd.is_empty() {
        let debug = env::var("FINDR_DEBUG").is_ok();
        let mut command = cmd.to_string();
        if ! command.contains("%(") {
            command = command + " %(path.path)";
        }
        if debug { println!("command '{}'", command); }
        let expr = percent_subst(&command);
        let fun = format!("fn cmd(path,date) {{ {} }}", expr);
        if debug { println!("expr '{}'", fun); }
        Some(fun)
    } else {
        None
    }
}

pub fn exec(cmd: &str) -> BoxResult<String> {
    let out = Command::new("/bin/sh")
        .arg("-c")
        .arg(cmd)
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
