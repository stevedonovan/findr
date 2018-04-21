extern crate walkdir;
extern crate rhai;
extern crate chrono;
extern crate chrono_english;
extern crate gitignore;
extern crate glob;
extern crate lapp;

const USAGE: &str = r#"
findr: find files and filter with expressions

  -n, --no-hidden look at hidden files and follow hidden dirs
  -g, --no-gitignore do not respect .gitignore
  -f, --follow-links follow symbolic links
  -m, --manual show more detailed help about findr

  <base-dir> (path) base directory to start traversal
  <filter-function> (default 'true') filter paths

If a filter is not provided and the base is not a dir, then
it is interpreted as a glob pattern searching from current dir.
If the glob does not start with '*', then:
  *  file-pattern becomes */file-pattern
  * .ext becomes *.ext
"#;

const MANUAL: &str = r#"
The filter-function is passed 'path', 'date' and 'mode'
path has the following fields:
  * is_file   is this path a file?
  * is_dir    is this path a directory?
  * is_exec   is this file executable?
  * is_write  is this path writeable?
  * size      size of file entry in bytes
  * ext       extension of file path
  * file_name file name part of path

And the method:
  * matches   path matches wildcard

date has the following methods:
  * before(datestr)  all files modified before this date
  * after(datestr)   all files modified after this date
  * on(datestr)      all files modified on this date (i.e. within 24h)
  * between(datestr,datestr)  all files modified between these dates

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
use glob::Pattern;

mod errors;
use errors::*;

mod preprocess;

use std::time::UNIX_EPOCH;
use std::fs::Metadata;
use std::path::{Path,PathBuf};
use std::io;
use std::io::Write;

// Windows will have to wait a bit...
use std::os::unix::fs::MetadataExt;

fn mode(m: &Metadata) -> i64 {
    (m.mode() & 0o777) as i64
}

fn file_name(entry: &DirEntry) -> &str {
    entry.file_name().to_str().unwrap_or(&"?")
}

#[derive(Clone)]
struct PathImpl {
    entry: DirEntry,
    metadata: Metadata,
    globs: Vec<Pattern>,
}

impl PathImpl {

    // this is seriously ugly. We need to pre-create this object
    // since it must look after the compiled glob patterns.
    // But entry needs a valid initialization...
    fn new(base: &Path, globs: Vec<Pattern>) -> BoxResult<PathImpl> {
        let entry = WalkDir::new(base).into_iter().next().unwrap()?;
        let metadata = entry.metadata()?;
        Ok(PathImpl {
            entry: entry, metadata: metadata,
            globs: globs
        })
    }

    fn set(&mut self, entry: DirEntry, metadata: Metadata) {
        self.entry = entry;
        self.metadata = metadata;
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

    fn matches(&mut self, idx: i64) -> bool {
        self.globs[idx as usize].matches_path(self.entry.path())
    }

    fn register(engine: &mut Engine) {
        engine.register_type::<PathImpl>();
        engine.register_get("is_file",PathImpl::is_file);
        engine.register_get("is_dir",PathImpl::is_dir);
        engine.register_get("is_exec",PathImpl::is_exec);
        engine.register_get("is_write",PathImpl::is_write);
        engine.register_get("size",PathImpl::size);
        engine.register_get("ext",PathImpl::ext);
        engine.register_get("file_name",PathImpl::file_name);
        engine.register_fn("matches",PathImpl::matches);
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

///// What we Ignore when walking through directories
struct GitIgnorePath {
    files: Vec<PathBuf>,
    depth: usize,
}

impl GitIgnorePath {
    fn new(path: &Path, depth: usize) -> GitIgnorePath {
        let full_path = path.canonicalize().unwrap();
        let gile = gitignore::File::new(&full_path).unwrap();
        GitIgnorePath{
            files: gile.included_files().unwrap(),
            depth: depth
        }
    }

    fn is_excluded(&self, path: &Path) -> bool {
        let full_path = path.canonicalize().unwrap();
        self.files.iter().filter(|&p| p == &full_path).count() == 0
    }
}

struct Ignore {
    use_gitignore: bool,
    hide_hidden: bool,
    maybe_gitignore: Option<GitIgnorePath>,
    at_start: bool,
}

impl Ignore {
    fn new(use_gitignore: bool, hide_hidden: bool) -> Ignore {
        Ignore {
            use_gitignore: use_gitignore,
            hide_hidden: hide_hidden,
            maybe_gitignore: None,
            at_start: true,
        }
    }

    // entries to be passed through filter...
    fn pass(&mut self, entry: &DirEntry) -> bool {
        let fname = file_name(entry);
        let path = entry.path();
        if self.use_gitignore {
            let mut dropped = false;
            // is this path excluded by current .gitignore?
            let ok = if let Some(ref gp) = self.maybe_gitignore {
                if entry.depth() < gp.depth { // no longer in dirs controlled by this .gitignore
                    dropped = true;
                    true
                } else {
                    let excluded = gp.is_excluded(path);
                    ! excluded
                }
            } else {
                true
            };
            if dropped { // avoid the borrow...
                self.maybe_gitignore = None;
            }
            if ! ok { // was excluded!
                return false;
            }
            if entry.file_type().is_dir() {
                // our task is to find if this directory contains .gitignore
                let depth = entry.depth();
                let gitignore_path = entry.path().join(".gitignore");
                // track any .gitignore files and the depth at which they occur
                if gitignore_path.exists() {
                    self.maybe_gitignore = Some(GitIgnorePath::new(&gitignore_path,depth+1));
                }
            }
        }
        // always pass first entry (could be . or .. which must not be hidden)
        // (allows us to pick up ./.gitignore or ../.gitignore)
        if self.at_start {
            self.at_start = false;
            return true;
        }
        if self.hide_hidden {
            ! fname.starts_with('.')
        } else {
            true
        }
    }
}

fn run() -> BoxResult<()> {
    let args = lapp::parse_args(USAGE);
    if args.get_bool("manual") {
        println!("{}",MANUAL);
        return Ok(());
    }
    let mut base = args.get_path("base-dir");
    let mut filter = args.get_string("filter-function");
    if filter == "true" { //* strictly speaking, if 2nd arg isn't present!
        if ! (base.exists() && base.is_dir()) {
            let glob = base.to_str().expect("can't get path as string").to_string();
            let wildcard = if ! glob.starts_with('*') {
                // .ext beomes *.ext, file becomes */file
                if glob.starts_with('.') {"*"} else {"*/"}
            } else {
                ""
            };
            filter = format!("path.matches(\"{}{}\")",wildcard,glob);
            base = PathBuf::from(".");
        }
    }
    let follow_hidden = args.get_bool("no-hidden");
    let no_gitignore = args.get_bool("no-gitignore");
    let follow_links = args.get_bool("follow-links");

    let (filter,patterns) = preprocess::create_filter(&filter,"filter","path,date,mode")?;

    // fire up Rhai, register our types and compile our filter
    let mut engine = Engine::new();
    let mut scope = Scope::new();

    PathImpl::register(&mut engine);
    DateImpl::register(&mut engine);

    engine.eval_with_scope::<()>(&mut scope, &filter)?;

    let walker = WalkDir::new(&base).follow_links(follow_links).into_iter();
    let mut ignore = Ignore::new(! no_gitignore, ! follow_hidden);
    let mut path_obj = PathImpl::new(&base, patterns)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for entry in walker.filter_entry(|e| ignore.pass(e) ) {
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
                        path_obj.set(entry,metadata);
                        let mut date_obj = DateImpl::new(tstamp);
                        let res = engine.call_fn::<_,_,bool>("filter",(&mut path_obj,&mut date_obj,&mut mode))?;
                        if res {
                            write!(out,"{}\n", path.display())?;
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
        std::process::exit(1);
    }
}
