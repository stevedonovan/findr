extern crate walkdir;
extern crate rhai;
extern crate chrono;
extern crate chrono_english;

const USAGE: &str = r#"
findr <base-dir> <filter-function>
where the filter-function is passed 'path', 'date' and 'mode'
path has the following fields:
  - is_file   is this path a file?
  - is_dir    is this path a directory?
  - is_exec   is this file executable?
  - is_write  is this path writeable?
  - size      size of file entry in bytes
  - ext       extension of file path
  - file_name file name part of path

date has the following methods:
  - before(datestr)  all files modified before this date
  - after(datestr)   all files modified after this date
  - on(datestr)      all files modified on this date (i.e. within 24h)
  - between(datestr,datestr)  all files modified between these dates

mode is the usual set of Unix permission bits.

For convenience, numbers may have size prefix (kb,mb,gb) and
date strings are as defined by chrono-english. "and","or" and "not"
may be used instead of "&&","||" or "!".

Set env var FINDR_US for American-style dates (m/d) - default is (d/m)

Examples:
$ findr . 'path.ext=="rs" && path.size > 1kb'
$ findr . 'path.is_file && date.before("1 jan")'
$ FINDR_US=1 findr . 'date.on("last 9/11")'

"#;

use walkdir::{DirEntry, WalkDir, WalkDirIterator};
use rhai::{Engine,Scope,RegisterFn};

mod errors;
use errors::*;

mod preprocess;

use std::time::UNIX_EPOCH;
use std::fs::Metadata;

// Windows will have to wait a bit...
use std::os::unix::fs::MetadataExt;

fn mode(m: &Metadata) -> i64 {
    (m.mode() & 0o777) as i64
}

//use std::fs::File
//use std::io;
//use std::io::prelude::*;

fn file_name(entry: &DirEntry) -> &str {
    entry.file_name().to_str().unwrap_or(&"?")
}

#[derive(Clone)]
struct PathImpl {
    entry: DirEntry,
    metadata: Metadata,
}

impl PathImpl {
    fn new(entry: DirEntry, metadata: Metadata) -> PathImpl {
        PathImpl { entry: entry, metadata: metadata }
    }

    fn is_file(&mut self) -> bool {
        self.metadata.is_file()
    }

    fn is_dir(&mut self) -> bool {
        self.metadata.is_dir()
    }

    fn is_exec(&mut self) -> bool {
        self.metadata.is_file() && mode(&self.metadata) & 0o100 != 0
    }

    fn is_write(&mut self) -> bool {
        mode(&self.metadata) & 0o200 != 0
    }

    // would uid and guid be useful?
    //fn uid(&mut self) -> i64

    fn size(&mut self) -> i64 {
        self.metadata.len() as i64
    }

    fn ext(&mut self) -> String {
        // a necessary ugliness?
        self.entry.path().extension()
            .map(|os| os.to_str().unwrap_or(&""))
            .unwrap_or(&"").into()
    }

    fn file_name(&mut self) -> String {
        file_name(&self.entry).into()
    }

    fn register(engine: &mut Engine) {
        engine.register_type::<PathImpl>();
        engine.register_get("is_file",PathImpl::is_file);
        engine.register_get("is_dir",PathImpl::is_dir);
        engine.register_get("is_exec",PathImpl::is_exec);
        engine.register_get("is_write",PathImpl::is_exec);
        engine.register_get("size",PathImpl::size);
        engine.register_get("ext",PathImpl::ext);
        engine.register_get("file_name",PathImpl::file_name);
    }

}

#[derive(Clone)]
struct DateImpl {
    tstamp: i64
}

impl DateImpl {
    fn new(tstamp: u64) -> DateImpl {
        DateImpl {tstamp: tstamp as i64}
    }

    fn before(&mut self, t: i64) -> bool {
        self.tstamp < t
    }

    fn after(&mut self, t: i64) -> bool {
        self.tstamp > t
    }

    fn between(&mut self, t1: i64, t2: i64) -> bool {
        self.tstamp > t1 && self.tstamp < t2
    }

    fn register(engine: &mut Engine) {
        engine.register_type::<DateImpl>();
        engine.register_fn("before",DateImpl::before);
        engine.register_fn("after",DateImpl::after);
        // alias for after
        engine.register_fn("since",DateImpl::after);
        engine.register_fn("between",DateImpl::between);
        // alias for between: preprocessor treats this specially
        engine.register_fn("on",DateImpl::between);
    }
}

fn run() -> BoxResult<()> {
    let mut args = std::env::args().skip(1);
    let base = args.next();
    let filter = args.next();
    if base.is_none() || filter.is_none() {
        println!("{}",USAGE);
        return Ok(());
    }
    let base = base.unwrap();
    let filter = filter.unwrap();
    let filter = preprocess::create_filter(&filter)?;

    // fire up Rhai, register our types and compile our filter
    let mut engine = Engine::new();
    let mut scope = Scope::new();

    PathImpl::register(&mut engine);
    DateImpl::register(&mut engine);

    engine.eval_with_scope::<()>(&mut scope, &filter)?;

    // we ignore the base dir itself
    // and we don't want to visit hidden directories (for now)
    let walker = WalkDir::new(&base).min_depth(1).into_iter();
    for entry in walker.filter_entry(|e| ! file_name(e).starts_with('.')) {
        // ugliness alert...these matches feel clumsy..
        match entry {
            Err(e) => eprintln!("bad entry {}",e),
            Ok(entry) => {
                let path = entry.path().to_path_buf();  // ewww
                match entry.metadata() {
                    Err(e) => eprintln!("no metadata for {}: {}",path.display(),e),
                    Ok(metadata) => {
                        let tstamp = metadata.modified()?
                            .duration_since(UNIX_EPOCH)?.as_secs();
                        let mut mode = mode(&metadata);
                        let mut path_obj = PathImpl::new(entry,metadata);
                        let mut date_obj = DateImpl::new(tstamp);
                        let res = engine.call_fn::<_,_,bool>("filter",(&mut path_obj,&mut date_obj,&mut mode))?;
                        if res {
                            println!("{}", path.display());
                        }
                    }
                }

            }
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}",e);
    }
}
