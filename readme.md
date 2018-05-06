# findr - finding files without flags

I am impressed by the sheer amount of functionality offered by the
Unix `find` command, but remain unable to remember how to use it
for anything other than the basics; otherwise I hit Google.
I don't have a very good memory for flags, but I do remember
_expressions_. `findr` is given exactly two arguments; the base directory
and a _filter expression_:

```
$ findr . 'path.ext=="rs" && path.size > 1kb'
$ findr . 'path.is_file && date.before("1 jan")'
$ findr . 'path.ext=="md" and date.after("last tuesday")'
```

The filter expression is passed `path`, `date` and `mode` and fairly arbitrary
expressions are supported, thanks to the very capable little embedded
language [rhai](https://github.com/jonathandturner/rhai). As a little
convenience, "and", "or" and "not" are understood, since these are
easier to type in a hurry.

`path` has the following fields:

  - `is_file`   is this path a file?
  - `is_dir`    is this path a directory?
  - `is_exec`   is this file executable?
  - `is_write`  is this path writeable?
  - `size`      size of file entry in bytes
  - `ext`       extension of file path
  - `file_name` file name part of path

There's also a `matches` method (e.g. `path.matches("*/readme.*")`),
and an ASCII-case-insensitive counterpart `matches_ignore_case`.

`date` has the following methods:

  - `before(datestr)`  all files modified before this date
  - `after(datestr)`   all files modified after this date
  - `between(datestr,datestr)`  all files modified between these dates
  - `on(datestr)` all files modified on _this day_

`mode` is just the usual Unix permission bits - expressions may
contain octal constants in Rust notation (e.g. `0o755`)

Numbers may have a size prefix (kb,mb,gb - not case-sensitive)
and date strings are interpreted by [chrono-english](https://github.com/stevedonovan/chrono-english).

Currently, `findr` ignores hidden directories and files excluded by `.gitignore`.
It has not been _entirely_ possible to do without flags!

```
~$ findr -h
findr: find files and filter with expressions

  -n, --no-hidden look at hidden files and follow hidden dirs
  -g, --no-gitignore do not respect .gitignore
  -f, --follow-links follow symbolic links
  -i, --case-insensitive do case-insensitive glob matches
  -m, --manual show more detailed help about findr

  <base-dir> (path) base directory to start traversal
  <filter-function> (default 'true') filter paths
```

By default, it speaks British English dates (i.e. not "9/11"),
unless the environment variable `FINDR_US` is defined.

Respecting `.gitignore` is something that makes your life easier if you are not particularly interested
in build artifacts. It is particularly useful in Rust projects because incremental compilation
generates a _lot_ of intermediate build artifacts. (if you _do_ need to override the defaults
then `-gn` will do the job.)

With `findr`, I can now finally answer the question "What the f*k did I do on Tuesday?":

```
~$ findr . 'date.on("last tues")'
./rust/repos/findr/src/errors.rs
./rust/scratch
./rust/scratch/over/test.over
./rust/scratch/over/type1.over
./rust/scratch/over/over.rs
./rust/scratch/over/empty.over
./rust/scratch/over/tuple.over
./rust/scratch/over/strs.over
./rust/scratch/over/main.over
./rust/scratch/over/id.over
./rust/scratch/over/numbers.over
./rust/scratch/over/strings.over
./rust/scratch/over/map.over
./rust/scratch/over/str.over
./rust/scanlex/src
./rust/scanlex/src/lib.rs
```
With the `-g` flag (ignore `.gitignore`) there are 538 files changed on that day!

To illustrate my point about flag madness, the exact equivalent of `findr . 'path.ext="rs"'` is:

```
find . -type d -path '*/\.*' -prune -o -not -name '.*' -type f -name '*.rs' -print
```

(I had to look that one up)

## Shortcut Filters

A feature inspired by the defaults of `ripgrep` is _shortcut filters_.

To quote the --manual:

```
If a filter is not provided and the base is not a dir, then
it is interpreted as a glob pattern searching from current dir.
If the glob does not start with '*', then:
  *  file-pattern becomes */file-pattern
  * .ext becomes *.ext
```

That is, `findr readme.md` is equivalent to `findr . 'path.matches("*/readme.md")`,
and `findr .c` is equivalent to `findr . 'path.matches("*.c")`.

The `--case-insensitive` (`-i`) flag will emit `matches_ignore_case` instead of `matches`,
so that `findr -i 'readme.*'` will match `README.TXT`, `README.md` or any of the many
variations found in the wild.

Furthermore we allow an additional condition after this implied glob
pattern. If it's `<` or `>`, then the meaning is a path size expression, otherwise
it's a time expression.

So `findr '.c after last tues'` will give me all C source files modified after last Tuesday,
and `findr '.doc > 256Kb'` gives all .doc files greater than 256Kb. (The single quotes
remain important to protect our expressions from shell wildcard expansion.)

To see what transformations that `findr` does on its filter, set the environment
variable `FINDR_DEBUG`.



